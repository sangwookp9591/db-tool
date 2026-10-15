use crate::{sql_error, SqliteSession};
use dbcore::*;
impl SqliteSession {
    pub(crate) async fn load_schema_graph(&self) -> Result<SchemaGraph, DbError> {
        self.with_connection(|c| {
            let mut graph = SchemaGraph::default();
            let mut stmt = c.prepare("SELECT name,type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(sql_error)?;
            let tables = stmt.query_map([], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).map_err(sql_error)?.collect::<Result<Vec<_>,_>>().map_err(sql_error)?;
            for (name,kind) in tables {
                let mut fields = c.prepare("SELECT p.name,p.type,p.pk,p.\"notnull\" OR (p.pk>0 AND (t.wr OR t.strict OR (upper(p.type)='INTEGER' AND (SELECT count(*) FROM pragma_table_info(?1) WHERE pk>0)=1))) FROM pragma_table_info(?1) p JOIN pragma_table_list() t ON t.name=?1 AND t.schema='main' ORDER BY p.cid").map_err(sql_error)?;
                let fields = fields.query_map([&name], |r| Ok(SchemaField {name:r.get(0)?,type_name:r.get(1)?,pk:r.get::<_,i64>(2)?>0,nullable:r.get::<_,i64>(3)?==0})).map_err(sql_error)?.collect::<Result<Vec<_>,_>>().map_err(sql_error)?;
                graph.entities.push(SchemaEntity {id:format!("table:{name}"),name:name.clone(),kind,fields});
                let mut fks = c.prepare("SELECT id,seq,\"table\",\"from\",\"to\" FROM pragma_foreign_key_list(?1) ORDER BY id,seq").map_err(sql_error)?;
                let rows = fks.query_map([&name], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<String>>(4)?))).map_err(sql_error)?;
                for row in rows {
                    let (id,seq,to,from_field,to_field)=row.map_err(sql_error)?;
                    let to_field = match to_field {Some(f)=>f,None=> c.query_row("SELECT name FROM pragma_table_info(?1) WHERE pk=?2",rusqlite::params![to,seq+1],|r|r.get(0)).map_err(sql_error)?};
                    let key=format!("{name}:{id}");
                    if let Some(rel) = graph.relations.last_mut().filter(|r|r.id==key) {rel.from_fields.push(from_field);rel.to_fields.push(to_field);} else {graph.relations.push(SchemaRelation {id:key,name:format!("FK {name} → {to}"),from:format!("table:{name}"),to:format!("table:{to}"),from_fields:vec![from_field],to_fields:vec![to_field],kind:"foreign_key".into()});}
                }
            }
            Ok(graph)
        }).await
    }
}
