use super::*;
fn ident(s: &str) -> String {
    format!("`{}`", s.replace('`', "``"))
}
impl NeoSession {
    pub(crate) async fn load_snapshot(&self) -> Result<SchemaSnapshot, DbError> {
        let mut snapshot = SchemaSnapshot {
            version: 1,
            dialect: "cypher".into(),
            objects: vec![],
        };
        for (kind,statement) in [("constraint","SHOW CONSTRAINTS YIELD name, createStatement RETURN name, createStatement"),("index","SHOW INDEXES YIELD name, createStatement, owningConstraint WHERE owningConstraint IS NULL RETURN name, createStatement")]{
            let r=self.internal(statement.into(),Default::default()).await?;
            if r.truncated{return Err(DbError::query("Schema exceeds snapshot limit"));}
            if let Some(t)=r.table{for row in t.rows{if let (Some(Value::Text(name)),Some(Value::Text(definition)))=(row.first(),row.get(1)){snapshot.objects.push(SchemaObject{namespace:String::new(),name:name.clone(),kind:kind.into(),definition:definition.clone(),columns:vec![]});}}}
        }
        for (kind, statement) in [
            ("label", "CALL db.labels() YIELD label RETURN label"),
            (
                "relationship",
                "CALL db.relationshipTypes() YIELD relationshipType RETURN relationshipType",
            ),
        ] {
            let r = self.internal(statement.into(), Default::default()).await?;
            if r.truncated {
                return Err(DbError::query("Schema exceeds snapshot limit"));
            }
            if let Some(t) = r.table {
                for row in t.rows {
                    if let Some(Value::Text(name)) = row.first() {
                        snapshot.objects.push(SchemaObject {
                            namespace: String::new(),
                            name: name.clone(),
                            kind: kind.into(),
                            definition: String::new(),
                            columns: vec![],
                        });
                    }
                }
            }
        }
        snapshot.normalize()?;
        Ok(snapshot)
    }
    pub(crate) fn render_schema(diff: &SnapshotDiff) -> Result<Vec<String>, DbError> {
        if diff.dialect != "cypher" {
            return Err(DbError::unsupported("Cypher snapshot required"));
        }
        let mut statements = vec![];
        for c in &diff.changes {
            let object = c
                .after
                .as_ref()
                .or(c.before.as_ref())
                .ok_or_else(|| DbError::query("Missing schema object"))?;
            if !["constraint", "index"].contains(&object.kind.as_str()) {
                return Err(DbError::unsupported(
                    "Labels and relationship types cannot be created without data",
                ));
            }
            if c.action == "remove" || c.action == "change" {
                let old = c
                    .before
                    .as_ref()
                    .ok_or_else(|| DbError::query("Missing previous schema"))?;
                statements.push(format!(
                    "DROP {} {}",
                    old.kind.to_uppercase(),
                    ident(&old.name)
                ));
            }
            if c.action == "add" || c.action == "change" {
                statements.push(object.definition.clone());
            }
        }
        Ok(statements)
    }
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
            return Err(DbError::query("Import requires 1..10000 rows"));
        }
        let path: Vec<String> = serde_json::from_str(&req.node_id)
            .map_err(|_| DbError::query("Select a label node"))?;
        if path.len() != 2 || path[0] != "label" {
            return Err(DbError::query("Select a label node"));
        }
        let mut rows = vec![];
        for edit in &req.edits {
            let RowEdit::Insert { values } = edit else {
                return Err(DbError::query("Import supports inserts only"));
            };
            let mut row = serde_json::Map::new();
            for (k, v) in values {
                if row.contains_key(k) {
                    return Err(DbError::query("Duplicate import property"));
                }
                let value = match v {
                    Value::Null => serde_json::Value::Null,
                    Value::Bool(v) => serde_json::json!(v),
                    Value::Int(s) => serde_json::json!(s
                        .parse::<i64>()
                        .map_err(|_| DbError::query("Invalid integer"))?),
                    Value::Float(FloatValue::Number(n)) if n.is_finite() => serde_json::json!(n),
                    Value::Text(s) => serde_json::json!(s),
                    _ => {
                        return Err(DbError::unsupported(
                            "Neo4j import supports scalar properties",
                        ))
                    }
                };
                row.insert(k.clone(), value);
            }
            rows.push(serde_json::Value::Object(row));
        }
        let sql = format!(
            "UNWIND $rows AS row CREATE (n:{}) SET n = row RETURN properties(n) AS imported",
            ident(&path[1])
        );
        let exec = ExecRequest {
            text: sql.clone(),
            params: serde_json::Map::from_iter([("rows".into(), serde_json::Value::Array(rows))]),
            read_only: false,
            row_limit: 100,
            graph_node_limit: 1,
            timeout_ms: req.timeout_ms,
            preview_sample_limit: 100,
        };
        if preview {
            return Ok(ImportOutput::Preview(
                self.preview_changes(&exec, cancel).await?,
            ));
        }
        // One UNWIND statement is one database transaction; cancellation drops its owned connection.
        self.bounded(&exec, cancel, false).await?;
        Ok(ImportOutput::Applied(EditResult {
            affected: vec![req.edits.len() as u64],
            statements: vec![sql],
        }))
    }
}
