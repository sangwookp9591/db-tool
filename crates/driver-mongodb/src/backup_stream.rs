use super::administration::cancelled;
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct Collection {
    name: String,
    options: serde_json::Value,
}
impl MongoSession {
    pub(super) async fn stream_backup(
        &self,
        cancel: CancelToken,
        sink: &dyn BackupSink,
    ) -> Result<BackupSummary, DbError> {
        self.administration_open()?;
        let mut summary = BackupSummary {
            format: "mongodb-ndjson-v2".into(),
            ..Default::default()
        };
        let mut collections = self
            .db()
            .list_collections()
            .await
            .map_err(|e| self.error(e))?;
        while collections.advance().await.map_err(|e| self.error(e))? {
            cancelled(&cancel)?;
            let spec = collections
                .deserialize_current()
                .map_err(|e| self.error(e))?;
            if spec.name.starts_with("system.") {
                continue;
            }
            let metadata = Collection {
                name: spec.name.clone(),
                options: serde_json::to_value(spec.options)
                    .map_err(|_| DbError::query("Invalid collection options"))?,
            };
            backup_record(
                sink,
                b'C',
                &serde_json::to_vec(&metadata)
                    .map_err(|_| DbError::query("Invalid collection metadata"))?,
            )
            .await?;
            let collection = self.db().collection::<Document>(&spec.name);
            let mut cursor = collection.find(doc! {}).await.map_err(|e| self.error(e))?;
            let mut count = 0;
            while cursor.advance().await.map_err(|e| self.error(e))? {
                cancelled(&cancel)?;
                let doc = cursor.deserialize_current().map_err(|e| self.error(e))?;
                backup_record(
                    sink,
                    b'D',
                    Bson::Document(doc)
                        .into_canonical_extjson()
                        .to_string()
                        .as_bytes(),
                )
                .await?;
                count += 1;
            }
            let mut cursor = collection.list_indexes().await.map_err(|e| self.error(e))?;
            let mut indexes = vec![];
            while cursor.advance().await.map_err(|e| self.error(e))? {
                let index = cursor.deserialize_current().map_err(|e| self.error(e))?;
                if index.options.as_ref().and_then(|o| o.name.as_deref()) != Some("_id_") {
                    indexes.push(index);
                }
            }
            backup_record(
                sink,
                b'I',
                &serde_json::to_vec(&indexes)
                    .map_err(|_| DbError::query("Invalid index metadata"))?,
            )
            .await?;
            summary.counts.push((spec.name, count));
        }
        backup_record(sink, b'Z', &[]).await?;
        Ok(summary)
    }
    pub(super) async fn stream_restore(
        &self,
        summary: &BackupSummary,
        source: &mut (dyn std::io::Read + Send),
        overwrite: bool,
        cancel: CancelToken,
    ) -> Result<(), DbError> {
        self.administration_open()?;
        if summary.format != "mongodb-ndjson-v2" {
            return Err(DbError::unsupported("Unsupported streaming MongoDB backup"));
        }
        if overwrite {
            return Err(DbError::unsupported("MongoDB overwrite is disabled; restore to a new empty database and switch connections"));
        }
        if !self
            .db()
            .list_collection_names()
            .await
            .map_err(|e| self.error(e))?
            .is_empty()
        {
            return Err(DbError::new(
                ErrorKind::Conflict,
                "Restore requires an empty database",
            ));
        }
        let mut created = vec![];
        let result = async {
            loop {
                cancelled(&cancel)?;
                let (tag, data) = read_backup_record(source)?
                    .ok_or_else(|| DbError::query("Incomplete MongoDB backup"))?;
                if tag == b'Z' && data.is_empty() {
                    if read_backup_record(source)?.is_some() {
                        return Err(DbError::query("Trailing backup records"));
                    }
                    break;
                }
                if tag != b'C' {
                    return Err(DbError::query("Expected collection record"));
                }
                let metadata: Collection = serde_json::from_slice(&data)
                    .map_err(|_| DbError::query("Invalid collection metadata"))?;
                if metadata.name.is_empty()
                    || metadata.name.starts_with("system.")
                    || metadata.name.contains('\0')
                    || created.contains(&metadata.name)
                {
                    return Err(DbError::query("Invalid or duplicate collection"));
                }
                let options: mongodb::options::CreateCollectionOptions =
                    serde_json::from_value(metadata.options)
                        .map_err(|_| DbError::query("Invalid collection options"))?;
                self.db()
                    .create_collection(&metadata.name)
                    .with_options(options)
                    .await
                    .map_err(|e| self.error(e))?;
                created.push(metadata.name.clone());
                let collection = self.db().collection::<Document>(&metadata.name);
                let mut batch = vec![];
                let mut size = 0;
                loop {
                    cancelled(&cancel)?;
                    let (tag, data) = read_backup_record(source)?
                        .ok_or_else(|| DbError::query("Incomplete collection"))?;
                    if tag == b'D' {
                        size += data.len();
                        let json: serde_json::Value = serde_json::from_slice(&data)
                            .map_err(|_| DbError::query("Invalid Extended JSON"))?;
                        let Bson::Document(doc) = Bson::try_from(json)
                            .map_err(|_| DbError::query("Invalid Extended JSON"))?
                        else {
                            return Err(DbError::query("Expected document"));
                        };
                        batch.push(doc);
                        if batch.len() >= 500 || size >= 4 * 1024 * 1024 {
                            collection
                                .insert_many(std::mem::take(&mut batch))
                                .await
                                .map_err(|e| self.error(e))?;
                            size = 0;
                        }
                    } else if tag == b'I' {
                        if !batch.is_empty() {
                            collection
                                .insert_many(batch)
                                .await
                                .map_err(|e| self.error(e))?;
                        }
                        let indexes: Vec<mongodb::IndexModel> = serde_json::from_slice(&data)
                            .map_err(|_| DbError::query("Invalid index metadata"))?;
                        if !indexes.is_empty() {
                            collection
                                .create_indexes(indexes)
                                .await
                                .map_err(|e| self.error(e))?;
                        }
                        break;
                    } else {
                        return Err(DbError::query("Invalid collection record sequence"));
                    }
                }
            }
            cancelled(&cancel)
        }
        .await;
        if result.is_err() {
            for name in created {
                if self
                    .db()
                    .collection::<Document>(&name)
                    .drop()
                    .await
                    .is_err()
                {
                    return Err(DbError::new(
                        ErrorKind::Connection,
                        "Restore failed and cleanup could not be confirmed",
                    ));
                }
            }
        }
        result
    }
}
