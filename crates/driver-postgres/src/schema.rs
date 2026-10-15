use crate::{path_id, PgSession};
use dbcore::*;
impl PgSession {
    pub(crate) async fn load_schema_graph(
        &self,
        scope: Option<&str>,
    ) -> Result<SchemaGraph, DbError> {
        let _guard = self.state.lock().await;
        let mut graph = SchemaGraph::default();
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
        let tables = self.client.query("SELECT c.oid::text,c.relname,c.relkind::text,n.nspname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relkind IN ('r','p','v','m') ORDER BY c.relname", &[&scope]).await.map_err(|e| self.error(e))?;
        for row in tables {
            let oid: String = row.get(0);
            let name: String = row.get(1);
            let kind: String = row.get(2);
            let fields = self.client.query("SELECT a.attname,format_type(a.atttypid,a.atttypmod),NOT a.attnotnull,EXISTS(SELECT 1 FROM pg_index i WHERE i.indrelid=a.attrelid AND i.indisprimary AND a.attnum=ANY(i.indkey)) FROM pg_attribute a WHERE a.attrelid=$1::text::oid AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum", &[&oid]).await.map_err(|e| self.error(e))?.into_iter().map(|r| SchemaField {name:r.get(0),type_name:r.get(1),nullable:r.get(2),pk:r.get(3)}).collect();
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
                name,
                kind: if kind == "v" || kind == "m" {
                    "view"
                } else {
                    "table"
                }
                .into(),
                fields,
            });
        }
        let relations = self.client.query("SELECT k.oid::text,k.conname,s.relname,tn.nspname,t.relname,ARRAY(SELECT a.attname::text FROM unnest(k.conkey) WITH ORDINALITY x(num,ord) JOIN pg_attribute a ON a.attrelid=k.conrelid AND a.attnum=x.num ORDER BY x.ord),ARRAY(SELECT a.attname::text FROM unnest(k.confkey) WITH ORDINALITY x(num,ord) JOIN pg_attribute a ON a.attrelid=k.confrelid AND a.attnum=x.num ORDER BY x.ord) FROM pg_constraint k JOIN pg_class s ON s.oid=k.conrelid JOIN pg_namespace sn ON sn.oid=s.relnamespace JOIN pg_class t ON t.oid=k.confrelid JOIN pg_namespace tn ON tn.oid=t.relnamespace WHERE k.contype='f' AND sn.nspname=$1 ORDER BY k.oid", &[&scope]).await.map_err(|e| self.error(e))?;
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
