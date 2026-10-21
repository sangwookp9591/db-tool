use crate::{path_id, PgSession};
use dbcore::*;
use std::collections::HashMap;

impl PgSession {
    pub(crate) async fn load_schema_graph(
        &self,
        scope: Option<&str>,
    ) -> Result<SchemaGraph, DbError> {
        let path = scope
            .filter(|s| s.starts_with('['))
            .map(crate::parse_path)
            .transpose()?;
        let scope = path
            .as_ref()
            .and_then(|p| p.get(1))
            .map(String::as_str)
            .or(scope)
            .unwrap_or("public");

        // Serialize catalog reads with other session operations, but keep Rust-side
        // grouping and graph construction outside the session lock.
        let (tables, columns, relations) = {
            let _guard = self.state.lock().await;
            let tables = self.client.query("SELECT c.oid::text,c.relname,c.relkind::text,n.nspname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relkind IN ('r','p','v','m') ORDER BY c.relname", &[&scope]).await.map_err(|e| self.error(e))?;
            let columns = self.client.query("SELECT c.relname,a.attname,format_type(a.atttypid,a.atttypmod),NOT a.attnotnull,(pk.attnum IS NOT NULL) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace JOIN pg_attribute a ON a.attrelid=c.oid LEFT JOIN (SELECT i.indrelid,unnest(i.indkey) AS attnum FROM pg_index i WHERE i.indisprimary) pk ON pk.indrelid=c.oid AND pk.attnum=a.attnum WHERE n.nspname=$1 AND c.relkind IN ('r','p','v','m') AND a.attnum>0 AND NOT a.attisdropped ORDER BY c.relname,a.attnum", &[&scope]).await.map_err(|e| self.error(e))?;
            let relations = self.client.query("SELECT k.oid::text,k.conname,s.relname,tn.nspname,t.relname,ARRAY(SELECT a.attname::text FROM unnest(k.conkey) WITH ORDINALITY x(num,ord) JOIN pg_attribute a ON a.attrelid=k.conrelid AND a.attnum=x.num ORDER BY x.ord),ARRAY(SELECT a.attname::text FROM unnest(k.confkey) WITH ORDINALITY x(num,ord) JOIN pg_attribute a ON a.attrelid=k.confrelid AND a.attnum=x.num ORDER BY x.ord) FROM pg_constraint k JOIN pg_class s ON s.oid=k.conrelid JOIN pg_namespace sn ON sn.oid=s.relnamespace JOIN pg_class t ON t.oid=k.confrelid JOIN pg_namespace tn ON tn.oid=t.relnamespace WHERE k.contype='f' AND sn.nspname=$1 ORDER BY k.oid", &[&scope]).await.map_err(|e| self.error(e))?;
            (tables, columns, relations)
        };

        let mut fields_by_table: HashMap<String, Vec<SchemaField>> = HashMap::new();
        for row in columns {
            let table: String = row.get(0);
            fields_by_table.entry(table).or_default().push(SchemaField {
                name: row.get(1),
                type_name: row.get(2),
                nullable: row.get(3),
                pk: row.get(4),
            });
        }

        let mut graph = SchemaGraph::default();
        for row in tables {
            let name: String = row.get(1);
            let kind: String = row.get(2);
            graph.entities.push(SchemaEntity {
                id: path_id(&[
                    "table",
                    scope,
                    if kind == "v" || kind == "m" {
                        "views"
                    } else {
                        "tables"
                    },
                    &name,
                ]),
                name: name.clone(),
                kind: if kind == "v" || kind == "m" {
                    "view"
                } else {
                    "table"
                }
                .into(),
                fields: fields_by_table.remove(&name).unwrap_or_default(),
            });
        }

        for r in relations {
            let from: String = r.get(2);
            let to_scope: String = r.get(3);
            let to: String = r.get(4);
            graph.relations.push(SchemaRelation {
                id: r.get(0),
                name: r.get(1),
                from: path_id(&["table", scope, "tables", &from]),
                to: path_id(&["table", &to_scope, "tables", &to]),
                from_fields: r.get(5),
                to_fields: r.get(6),
                kind: "foreign_key".into(),
            });
        }
        Ok(graph)
    }
}
