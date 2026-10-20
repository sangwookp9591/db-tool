use super::*;
impl MongoSession {
    pub(crate) async fn load_snapshot(&self) -> Result<SchemaSnapshot, DbError> {
        let mut snapshot = SchemaSnapshot {
            version: 1,
            dialect: "mongodb".into(),
            objects: vec![],
        };
        let mut cursor = self
            .db()
            .list_collections()
            .await
            .map_err(|e| self.error(e))?;
        while cursor.advance().await.map_err(|e| self.error(e))? {
            if snapshot.objects.len() > 10000 {
                return Err(DbError::query("MongoDB snapshot exceeds object cap"));
            }
            let spec = cursor.deserialize_current().map_err(|e| self.error(e))?;
            let is_view = spec.collection_type == mongodb::results::CollectionType::View;
            let opts = serde_json::to_value(&spec.options)
                .map_err(|_| DbError::query("Invalid collection options"))?;
            snapshot.objects.push(SchemaObject {
                namespace: self.database.clone(),
                name: spec.name.clone(),
                kind: if is_view {"view"} else {"collection"}.into(),
                definition: serde_json::json!({"create":spec.name,"options":opts}).to_string(),
                columns: vec![],
            });
            if is_view {continue;}
            let mut indexes = self
                .db()
                .collection::<Document>(&spec.name)
                .list_indexes()
                .await
                .map_err(|e| self.error(e))?;
            while indexes.advance().await.map_err(|e| self.error(e))? {
                let index = indexes.deserialize_current().map_err(|e| self.error(e))?;
                let name = index
                    .options
                    .as_ref()
                    .and_then(|o| o.name.as_ref())
                    .ok_or_else(|| DbError::query("Index has no name"))?;
                let definition=serde_json::json!({"createIndexes":spec.name,"indexes":[serde_json::to_value(&index).map_err(|_|DbError::query("Invalid index metadata"))?]}).to_string();
                snapshot.objects.push(SchemaObject {
                    namespace: self.database.clone(),
                    name: format!("{}.{name}", spec.name),
                    kind: "index".into(),
                    definition,
                    columns: vec![],
                });
                if snapshot.objects.len() > 10000 {
                    return Err(DbError::query("MongoDB snapshot exceeds object cap"));
                }
            }
        }
        snapshot.normalize()?;
        Ok(snapshot)
    }
    pub(crate) fn render_schema(diff: &SnapshotDiff) -> Result<Vec<String>, DbError> {
        if diff.dialect != "mongodb" {
            return Err(DbError::unsupported("MongoDB snapshot required"));
        }
        let mut commands = vec![];
        for c in &diff.changes {
            let o = c
                .after
                .as_ref()
                .or(c.before.as_ref())
                .ok_or_else(|| DbError::query("Missing schema object"))?;
            if o.kind != "index" {
                return Err(DbError::unsupported("Only MongoDB index DDL is supported"));
            }
            if c.action == "remove" || c.action == "change" {
                let old = c
                    .before
                    .as_ref()
                    .ok_or_else(|| DbError::query("Missing prior index"))?;
                let def: serde_json::Value = serde_json::from_str(&old.definition)
                    .map_err(|_| DbError::query("Invalid index definition"))?;
                let collection = def["createIndexes"]
                    .as_str()
                    .ok_or_else(|| DbError::query("Missing collection"))?;
                let name = def["indexes"][0]["name"]
                    .as_str()
                    .ok_or_else(|| DbError::query("Missing index name"))?;
                commands
                    .push(serde_json::json!({"dropIndexes":collection,"index":name}).to_string());
            }
            if c.action == "add" || c.action == "change" {
                commands.push(o.definition.clone());
            }
        }
        Ok(commands)
    }
}
impl MongoSession {
    pub(crate) async fn import_batch(
        &self,
        req: &EditRequest,
        preview: bool,
        cancel: CancelToken,
    ) -> Result<ImportOutput, DbError> {
        if req.create_table.is_some() {
            return Err(DbError::unsupported(
                "Explicit table creation requires a SQL driver",
            ));
        }
        if req.edits.is_empty() || req.edits.len() > IMPORT_MAX_ROWS {
            return Err(DbError::query("Import requires 1..10000 inserts"));
        }
        let path = parse_path(&req.node_id)?;
        if path.len() != 3 || path[0] != "collection" || path[1] != self.database {
            return Err(DbError::query(
                "Select a collection in the current database",
            ));
        }
        let mut documents = vec![];
        for row in &req.edits {
            let RowEdit::Insert { values } = row else {
                return Err(DbError::query("Import supports inserts only"));
            };
            let mut doc = Document::new();
            for (key, value) in values {
                if doc.contains_key(key) || key.contains('\0') {
                    return Err(DbError::query("Invalid or duplicate field name"));
                }
                let value = match value {
                    Value::Null => Bson::Null,
                    Value::Bool(v) => Bson::Boolean(*v),
                    Value::Int(v) => {
                        Bson::Int64(v.parse().map_err(|_| DbError::query("Invalid integer"))?)
                    }
                    Value::Float(FloatValue::Number(v)) if v.is_finite() => Bson::Double(*v),
                    Value::Text(v) => Bson::String(v.clone()),
                    Value::Json(v) => Bson::try_from(
                        serde_json::from_str::<serde_json::Value>(v)
                            .map_err(|_| DbError::query("Invalid JSON"))?,
                    )
                    .map_err(|_| DbError::query("Invalid Extended JSON"))?,
                    _ => {
                        return Err(DbError::unsupported(
                            "MongoDB import requires scalar fields",
                        ))
                    }
                };
                doc.insert(key, value);
            }
            documents.push(doc);
        }
        let hello = self
            .db()
            .run_command(doc! {"hello":1})
            .await
            .map_err(|e| self.error(e))?;
        if !hello.contains_key("setName") && hello.get_str("msg").ok() != Some("isdbgrid") {
            return Err(DbError::unsupported("Atomic MongoDB import requires a replica set or mongos; standalone servers are not modified"));
        }
        let mut session = self
            .client
            .start_session()
            .await
            .map_err(|e| self.error(e))?;
        session
            .start_transaction()
            .await
            .map_err(|e| self.error(e))?;
        let collection = self.db().collection::<Document>(&path[2]);
        let work = collection.insert_many(documents).session(&mut session);
        let result = tokio::select! {_ = cancel.cancelled()=>Err(DbError::new(ErrorKind::Cancelled,"Import cancelled")),_ = tokio::time::sleep(std::time::Duration::from_millis(req.timeout_ms.max(1)))=>Err(DbError::new(ErrorKind::Timeout,"Import timed out")),r=work=>r.map_err(|e|self.error(e)).map(|_|())};
        if preview || result.is_err() || cancel.is_cancelled() {
            session
                .abort_transaction()
                .await
                .map_err(|e| self.error(e))?;
        } else {
            session
                .commit_transaction()
                .await
                .map_err(|e| self.error(e))?;
        }
        result?;
        if cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "Import cancelled"));
        }
        if preview {
            let mut statement =
                StatementPreview::new("insertMany (values omitted)".into(), StatementKind::Insert);
            statement.affected = Some(req.edits.len() as u64);
            Ok(ImportOutput::Preview(ChangePreview {
                statements: vec![statement],
                committed: false,
                notes: vec!["MongoDB transaction aborted".into()],
            }))
        } else {
            Ok(ImportOutput::Applied(EditResult {
                affected: vec![req.edits.len() as u64],
                statements: vec!["insertMany (values omitted)".into()],
            }))
        }
    }
}
