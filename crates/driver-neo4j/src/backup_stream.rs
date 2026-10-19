use super::administration::cancelled;
use super::administration::{ident, raw_literal, string};
use super::*;
use serde::{Deserialize, Serialize};
type BinaryParams = Vec<(String, Vec<u8>)>;
type NodeRecord = (String, String, String, BinaryParams);
type RelationshipRecord = (String, String, String, String, BinaryParams);
#[derive(Serialize, Deserialize)]
struct Statement {
    sql: String,
    params: Vec<(String, Vec<u8>)>,
}
impl NeoSession {
    pub(super) async fn stream_backup(
        &self,
        cancel: CancelToken,
        sink: &dyn BackupSink,
    ) -> Result<BackupSummary, DbError> {
        let schema = self
            .load_snapshot()
            .await?
            .objects
            .into_iter()
            .filter(|o| ["constraint", "index"].contains(&o.kind.as_str()))
            .map(|o| (o.kind, o.name, o.definition))
            .collect::<Vec<_>>();
        backup_record(
            sink,
            b'S',
            &serde_json::to_vec(&schema).map_err(|_| DbError::query("Invalid graph schema"))?,
        )
        .await?;
        let graph =
            neo4rs::Graph::connect(self.operation_config(3600000)).map_err(|e| self.error(e))?;
        let mut tx = graph.start_txn().await.map_err(|e| self.error(e))?;
        let result = async {
            let mut nodes = 0;
            let mut relationships = 0;
            let mut stream = tx
                .execute(query("MATCH (n) RETURN n ORDER BY id(n)"))
                .await
                .map_err(|e| self.error(e))?;
            while let Some(row) = stream.next(&mut tx).await.map_err(|e| self.error(e))? {
                cancelled(&cancel)?;
                let raw: BoltType = row.get("n").map_err(|_| DbError::query("Invalid node"))?;
                let BoltType::Node(n) = &raw else {
                    return Err(DbError::query("Expected node"));
                };
                let mut params = vec![];
                let properties = raw_literal(&BoltType::Map(n.properties.clone()), &mut params)?;
                let labels = n
                    .labels
                    .iter()
                    .map(|l| match l {
                        BoltType::String(l) => Ok(ident(&l.value)),
                        _ => Err(DbError::query("Invalid node label")),
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join(":");
                let record = (n.id.value.to_string(), labels, properties, params);
                backup_record(
                    sink,
                    b'N',
                    &serde_json::to_vec(&record)
                        .map_err(|_| DbError::query("Cannot encode graph node"))?,
                )
                .await?;
                nodes += 1;
            }
            let mut stream = tx
                .execute(query("MATCH ()-[r]->() RETURN r ORDER BY id(r)"))
                .await
                .map_err(|e| self.error(e))?;
            while let Some(row) = stream.next(&mut tx).await.map_err(|e| self.error(e))? {
                cancelled(&cancel)?;
                let raw: BoltType = row
                    .get("r")
                    .map_err(|_| DbError::query("Invalid relationship"))?;
                let BoltType::Relation(r) = &raw else {
                    return Err(DbError::query("Expected relationship"));
                };
                let mut params = vec![];
                let properties = raw_literal(&BoltType::Map(r.properties.clone()), &mut params)?;
                let record = (
                    r.start_node_id.value.to_string(),
                    r.end_node_id.value.to_string(),
                    ident(&r.typ.value),
                    properties,
                    params,
                );
                backup_record(
                    sink,
                    b'R',
                    &serde_json::to_vec(&record)
                        .map_err(|_| DbError::query("Cannot encode relationship"))?,
                )
                .await?;
                relationships += 1;
            }
            backup_record(sink, b'Z', &[]).await?;
            Ok(BackupSummary {
                format: "neo4j-cypher-v2".into(),
                counts: vec![
                    ("nodes".into(), nodes),
                    ("relationships".into(), relationships),
                ],
                warnings: vec![],
            })
        }
        .await;
        tx.rollback().await.map_err(|e| self.error(e))?;
        result
    }
    pub(super) async fn stream_restore(
        &self,
        summary: &BackupSummary,
        source: &mut (dyn std::io::Read + Send),
        overwrite: bool,
        cancel: CancelToken,
    ) -> Result<(), DbError> {
        if summary.format != "neo4j-cypher-v2" {
            return Err(DbError::unsupported("Unsupported streaming Neo4j backup"));
        }
        if overwrite {
            return Err(DbError::unsupported(
                "Neo4j overwrite is disabled; restore to an empty database and switch connections",
            ));
        }
        let existing = self
            .internal(
                "MATCH (n) RETURN count(n) AS count".into(),
                Default::default(),
            )
            .await?;
        if existing
            .table
            .as_ref()
            .and_then(|t| t.rows.first())
            .and_then(|r| r.first())
            != Some(&Value::Int("0".into()))
        {
            return Err(DbError::new(
                ErrorKind::Conflict,
                "Restore requires an empty graph",
            ));
        }
        if self.load_snapshot().await?.objects.iter().any(|o| {
            o.kind == "constraint" || o.kind == "index" && !o.definition.contains("LOOKUP")
        }) {
            return Err(DbError::new(
                ErrorKind::Conflict,
                "Restore requires an empty schema",
            ));
        }
        let Some((b'S', data)) = read_backup_record(source)? else {
            return Err(DbError::query("Expected graph schema"));
        };
        let schema: Vec<(String, String, String)> =
            serde_json::from_slice(&data).map_err(|_| DbError::query("Invalid graph schema"))?;
        let graph =
            neo4rs::Graph::connect(self.operation_config(3600000)).map_err(|e| self.error(e))?;
        let marker = format!(
            "__axdb_restore_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| DbError::internal("Clock unavailable"))?
                .as_nanos()
        );
        let label = ident(&marker);
        let key = ident(&marker);
        let mut created = vec![];
        // The temporary lookup index keeps node-id resolution on the server, not in a host-sized map.
        let result=async {
            for (kind,name,sql) in &schema {cancelled(&cancel)?;if sql.contains("LOOKUP"){continue;}graph.run(query(sql)).await.map_err(|e|self.error(e))?;created.push((kind.clone(),name.clone()));}
            graph.run(query(&format!("CREATE CONSTRAINT {label} FOR (n:{label}) REQUIRE n.{key} IS UNIQUE"))).await.map_err(|e|self.error(e))?;
            created.push(("constraint".into(),marker.clone()));
            let mut tx=graph.start_txn().await.map_err(|e|self.error(e))?;
            let work=async {
                loop {
                    cancelled(&cancel)?;
                    let (tag,data)=read_backup_record(source)?.ok_or_else(||DbError::query("Incomplete graph backup"))?;
                    let statement=match tag {
                        b'N'=>{
                            let (id,labels,properties,params):NodeRecord=serde_json::from_slice(&data).map_err(|_|DbError::query("Invalid node record"))?;
                            // A generated marker cannot overwrite a source property, even in a crafted archive.
                            if properties.contains(&key)||labels.contains(&label){return Err(DbError::query("Reserved restore marker collision"));}
                            Statement{sql:format!("CREATE (n{} {properties}) SET n:{label}, n.{key}={}",if labels.is_empty(){String::new()}else{format!(":{labels}")},string(&id)),params}
                        },
                        b'R'=>{
                            let (a,b,typ,properties,params):RelationshipRecord=serde_json::from_slice(&data).map_err(|_|DbError::query("Invalid relationship record"))?;
                            Statement{sql:format!("MATCH (a:{label} {{{key}:{}}}),(b:{label} {{{key}:{}}}) CREATE (a)-[r:{typ} {properties}]->(b) RETURN count(r) AS created",string(&a),string(&b)),params}
                        },
                        b'Z' if data.is_empty()=>{if read_backup_record(source)?.is_some(){return Err(DbError::query("Trailing graph backup records"));}break;},
                        _=>return Err(DbError::query("Invalid graph backup record sequence")),
                    };
                    let mut q=query(&statement.sql);for (key,value) in statement.params{q=q.param(&key,BoltType::Bytes(neo4rs::BoltBytes::new(value.into())));}
                    if tag==b'R'{
                        let mut rows=tx.execute(q).await.map_err(|e|self.error(e))?;
                        let row=rows.next(&mut tx).await.map_err(|e|self.error(e))?.ok_or_else(||DbError::query("Missing relationship result"))?;
                        if row.get::<i64>("created").map_err(|_|DbError::query("Invalid relationship result"))?!=1{return Err(DbError::query("Dangling or duplicate node reference in backup"));}
                        if rows.next(&mut tx).await.map_err(|e|self.error(e))?.is_some(){return Err(DbError::query("Invalid relationship result sequence"));}
                    }else{tx.run(q).await.map_err(|e|self.error(e))?;}
                }
                tx.run(query(&format!("MATCH (n:{label}) REMOVE n:{label}, n.{key}"))).await.map_err(|e|self.error(e))?;
                cancelled(&cancel)
            };
            let work=tokio::select!{r=work=>r,_=cancel.cancelled()=>Err(DbError::new(ErrorKind::Cancelled,"Restore cancelled"))};
            if let Err(error)=work {if !matches!(tokio::time::timeout(Duration::from_secs(3),tx.rollback()).await,Ok(Ok(()))){return Err(DbError::new(ErrorKind::Connection,"Restore rollback could not be confirmed"));}return Err(error);}
            tx.commit().await.map_err(|e|self.error(e))?;
            Ok(())
        }.await;
        for (kind, name) in created.into_iter().rev() {
            if result.is_err() || name == marker {
                graph
                    .run(query(&format!(
                        "DROP {} {} IF EXISTS",
                        kind.to_uppercase(),
                        ident(&name)
                    )))
                    .await
                    .map_err(|_| {
                        DbError::new(ErrorKind::Connection, "Restore schema cleanup failed")
                    })?;
            }
        }
        result
    }
}
