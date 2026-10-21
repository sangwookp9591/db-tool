use crate::{node, parse_path, type_name, MongoSession};
use dbcore::*;
use mongodb::bson::{doc, oid::ObjectId, Bson, Document};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};

// shortcut: samples describe observed shapes, use validators/index metadata when complete schemas are needed.
const COLLECTION_LIMIT: usize = 100;
const SAMPLE_LIMIT: i64 = 200;
const REFERENCE_SAMPLES: usize = 5;
const FIELD_LIMIT: usize = 2_000;
const REFERENCE_LIMIT: usize = 100;
const OP_TIMEOUT: Duration = Duration::from_secs(3);
const GRAPH_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Default)]
struct Samples {
    database: String,
    fields: BTreeMap<String, (BTreeSet<String>, usize)>,
    references: BTreeMap<(String, String), Vec<ObjectId>>,
}
impl Samples {
    fn document(&mut self, document: &Document) {
        let mut seen = BTreeSet::new();
        self.walk(document, "", 0, &mut seen);
        for field in seen {
            if let Some((_, count)) = self.fields.get_mut(&field) {
                *count += 1;
            }
        }
    }
    fn walk(
        &mut self,
        document: &Document,
        prefix: &str,
        depth: usize,
        seen: &mut BTreeSet<String>,
    ) {
        if depth >= 32 {
            return;
        }
        if let (Ok(target), Some(Bson::ObjectId(id))) =
            (document.get_str("$ref"), document.get("$id"))
            && !prefix.is_empty()
            && (!document.contains_key("$db")
                || document.get_str("$db").ok() == Some(self.database.as_str()))
        {
            self.reference(&format!("{prefix}.$id"), target, *id);
        }
        for (key, value) in document {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            if self.fields.len() >= FIELD_LIMIT && !self.fields.contains_key(&path) {
                continue;
            }
            self.fields
                .entry(path.clone())
                .or_default()
                .0
                .insert(type_name(value).to_ascii_lowercase());
            seen.insert(path.clone());
            if let Bson::ObjectId(id) = value
                && let Some(base) = key
                    .strip_suffix("Id")
                    .or_else(|| key.strip_suffix("_id"))
                    .filter(|s| !s.is_empty())
            {
                self.reference(&path, base, *id);
                self.reference(&path, &format!("{base}s"), *id);
            }
            match value {
                Bson::Document(child) => {
                    self.walk(child, &path, depth + 1, seen);
                }
                Bson::Array(values) => {
                    for child in values
                        .iter()
                        .take(SAMPLE_LIMIT as usize)
                        .filter_map(Bson::as_document)
                    {
                        self.walk(child, &path, depth + 1, seen);
                    }
                }
                _ => {}
            }
        }
    }
    fn reference(&mut self, path: &str, target: &str, id: ObjectId) {
        let key = (path.into(), target.into());
        if self.references.len() >= REFERENCE_LIMIT && !self.references.contains_key(&key) {
            return;
        }
        let values = self.references.entry(key).or_default();
        if values.len() < REFERENCE_SAMPLES && !values.contains(&id) {
            values.push(id);
        }
    }
    fn fields(&self, count: usize) -> Vec<SchemaField> {
        self.fields
            .iter()
            .map(|(name, (types, present))| SchemaField {
                name: name.clone(),
                type_name: types.iter().cloned().collect::<Vec<_>>().join("|"),
                pk: name == "_id",
                nullable: *present < count || types.contains("null"),
            })
            .collect()
    }
}

fn collection_id(database: &str, name: &str) -> String {
    node(
        &["collection", database, name],
        name,
        "collection",
        Icon::Collection,
        true,
        None,
    )
    .id
}

// Only the direct equality form has unambiguous local/foreign fields in the view output.
fn view_lookups(pipeline: &[Document]) -> Vec<(&str, &str, &str)> {
    pipeline
        .iter()
        .filter_map(|stage| {
            let lookup = stage.get_document("$lookup").ok()?;
            Some((
                lookup.get_str("from").ok()?,
                lookup.get_str("localField").ok()?,
                lookup.get_str("foreignField").ok()?,
            ))
        })
        .collect()
}

impl MongoSession {
    pub(crate) async fn load_schema_graph(
        &self,
        scope: Option<&str>,
    ) -> Result<SchemaGraph, DbError> {
        let database = match scope {
            None => self.database.clone(),
            Some(scope) => {
                let path = parse_path(scope)?;
                match path
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    ["database", database] | ["collection", database, _] => (*database).to_owned(),
                    _ => return Err(DbError::query("Invalid MongoDB schema scope")),
                }
            }
        };
        if self.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(DbError::new(ErrorKind::Connection, "Session is closed"));
        }
        let session = self.clone();
        // Let the SDK drain in-flight replies if the caller's deadline expires.
        let task = tokio::spawn(async move { session.schema_graph_inner(&database).await });
        tokio::time::timeout(GRAPH_TIMEOUT, task)
            .await
            .map_err(|_| DbError::new(ErrorKind::Timeout, "MongoDB schema graph timed out"))?
            .map_err(|_| DbError::internal("MongoDB schema graph task failed"))?
    }
    async fn schema_graph_inner(&self, database: &str) -> Result<SchemaGraph, DbError> {
        let deadline = Instant::now() + GRAPH_TIMEOUT;
        let remaining = || {
            deadline
                .checked_duration_since(Instant::now())
                .map(|d| d.min(OP_TIMEOUT))
                .ok_or_else(|| DbError::new(ErrorKind::Timeout, "MongoDB schema graph timed out"))
        };
        let db = self.client.database(database);
        let mut cursor = db
            .list_collections()
            .filter(doc! {"name":{"$not":{"$regex":"^system\\."}}})
            .batch_size((COLLECTION_LIMIT + 1) as u32)
            .await
            .map_err(|e| self.error(e))?;
        let mut specs = Vec::new();
        while cursor.advance().await.map_err(|e| self.error(e))? {
            remaining()?;
            if specs.len() == COLLECTION_LIMIT {
                return Err(DbError::unsupported(
                    "MongoDB schema graph supports at most 100 collections/views per database",
                ));
            }
            specs.push(cursor.deserialize_current().map_err(|e| self.error(e))?);
        }
        specs.sort_by(|a, b| a.name.cmp(&b.name));
        let collections = specs
            .iter()
            .filter(|s| s.options.view_on.is_none())
            .map(|s| s.name.as_str())
            .collect::<BTreeSet<_>>();
        let names = specs
            .iter()
            .map(|s| s.name.as_str())
            .collect::<BTreeSet<_>>();
        let mut graph = SchemaGraph::default();
        for spec in &specs {
            let collection = db.collection::<Document>(&spec.name);
            let mut cursor = collection
                .aggregate(vec![doc! {"$sample":{"size":SAMPLE_LIMIT}}])
                .max_time(remaining()?)
                .batch_size(SAMPLE_LIMIT as u32)
                .await
                .map_err(|e| self.error(e))?;
            let mut samples = Samples {
                database: database.into(),
                ..Default::default()
            };
            let mut count = 0;
            while count < SAMPLE_LIMIT as usize
                && cursor.advance().await.map_err(|e| self.error(e))?
            {
                remaining()?;
                let document = cursor.deserialize_current().map_err(|e| self.error(e))?;
                samples.document(&document);
                count += 1;
            }
            let from = collection_id(database, &spec.name);
            graph.entities.push(SchemaEntity {
                id: from.clone(),
                name: spec.name.clone(),
                kind: if spec.options.view_on.is_some() {
                    "view"
                } else {
                    "collection"
                }
                .into(),
                fields: samples.fields(count),
            });
            for ((field, target), ids) in samples.references {
                if !collections.contains(target.as_str()) {
                    continue;
                }
                let found = db
                    .collection::<Document>(&target)
                    .find_one(doc! {"_id":{"$in":ids}})
                    .projection(doc! {"_id":1})
                    .max_time(remaining()?)
                    .await
                    .map_err(|e| self.error(e))?;
                if found.is_some() {
                    graph.relations.push(SchemaRelation {
                        id: format!("inferred:{from}:{field}:{target}"),
                        name: format!("inferred_reference · {field}"),
                        from: from.clone(),
                        to: collection_id(database, &target),
                        from_fields: vec![field],
                        to_fields: vec!["_id".into()],
                        kind: "inferred_reference".into(),
                    });
                }
            }
            if let Some(pipeline) = &spec.options.pipeline {
                for (index, (target, local, foreign)) in
                    view_lookups(pipeline).into_iter().enumerate()
                {
                    if !names.contains(target) {
                        continue;
                    }
                    graph.relations.push(SchemaRelation {
                        id: format!("lookup:{from}:{index}"),
                        name: "view_lookup".into(),
                        from: from.clone(),
                        to: collection_id(database, target),
                        from_fields: vec![local.into()],
                        to_fields: vec![foreign.into()],
                        kind: "view_lookup".into(),
                    });
                }
            }
        }
        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn infers_nested_unions_nullable_fields_and_references() {
        let id = ObjectId::new();
        let mut samples = Samples::default();
        samples.document(&doc! {"_id":id,"userId":id,"user_id":id,"nested":{"name":"a"},"items":[{"n":1},{"n":"two"}],"ref":{"$ref":"users","$id":id}});
        samples.document(&doc! {"nested":{"name":Bson::Null}});
        let fields = samples.fields(2);
        let name = fields.iter().find(|f| f.name == "nested.name").unwrap();
        assert_eq!(name.type_name, "null|string");
        assert!(name.nullable);
        assert_eq!(
            fields
                .iter()
                .find(|f| f.name == "items.n")
                .unwrap()
                .type_name,
            "int32|string"
        );
        assert_eq!(
            samples.references[&("userId".into(), "users".into())],
            vec![id]
        );
        assert_eq!(
            samples.references[&("user_id".into(), "user".into())],
            vec![id]
        );
        assert!(samples
            .references
            .contains_key(&("ref.$id".into(), "users".into())));
        assert!(!samples.references.keys().any(|(field, _)| field == "_id"));
    }
    #[test]
    fn dbrefs_respect_database_and_reference_sample_limit() {
        let mut samples = Samples {
            database: "test".into(),
            ..Default::default()
        };
        for _ in 0..10 {
            let d = doc! {"same":{"$ref":"users","$id":ObjectId::new(),"$db":"test"},"other":{"$ref":"users","$id":ObjectId::new(),"$db":"private"}};
            samples.document(&d);
        }
        assert_eq!(
            samples.references[&("same.$id".into(), "users".into())].len(),
            REFERENCE_SAMPLES
        );
        assert!(!samples
            .references
            .keys()
            .any(|(field, _)| field == "other.$id"));
        assert_eq!(
            view_lookups(&[
                doc! {"$lookup":{"from":"users","localField":"userId","foreignField":"_id","as":"user"}}
            ]),
            vec![("users", "userId", "_id")]
        );
    }
}
