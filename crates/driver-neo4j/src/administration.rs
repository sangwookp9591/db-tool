use super::*;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
struct Dump {
    schema: Vec<(String, String, String)>,
    cypher: String,
    #[serde(default)]
    bytes_params: Vec<(String, Vec<u8>)>,
}
pub(super) fn ident(s: &str) -> String {
    format!("`{}`", s.replace('`', "``"))
}
pub(super) fn string(s: &str) -> String {
    format!(
        "'{}'",
        s.replace('\\', "\\\\")
            .replace('\'', "\\'")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}
fn literal(v: &Value) -> Result<String, DbError> {
    Ok(match v {
    Value::Null=>"null".into(),Value::Bool(b)=>b.to_string(),Value::Int(n)=>n.parse::<i64>().map_err(|_|DbError::query("Invalid integer"))?.to_string(),Value::Float(FloatValue::Number(n)) if n.is_finite()=>format!("{n:e}"),Value::Text(s)=>string(s),
    Value::Date(s)=>format!("date({})",string(s)),Value::DateTime(s)=>format!("localdatetime({})",string(s)),Value::DateTimeTz(s)=>format!("datetime({})",string(s)),Value::Time(s)=>format!("{}({})",if s.contains(['+','-']){"time"}else{"localtime"},string(s)),Value::Duration(s)=>format!("duration({})",string(s)),
    Value::List(values)=>format!("[{}]",values.iter().map(literal).collect::<Result<Vec<_>,_>>()?.join(",")),
    Value::Map(values)=>format!("{{{}}}",values.iter().map(|(k,v)|Ok(format!("{}:{}",ident(k),literal(v)?))).collect::<Result<Vec<_>,DbError>>()?.join(",")),
    Value::Point{srid,x,y,z}=>format!("point({{srid:{srid},x:{x:e},y:{y:e}{}}})",z.map(|z|format!(",z:{z:e}")).unwrap_or_default()),
    Value::Float(FloatValue::Special(s)) if ["NaN","Infinity","-Infinity"].contains(&s.as_str())=>format!("toFloat({})",string(s)),
    _=>return Err(DbError::unsupported("Native Cypher backup cannot round-trip this property type; no partial backup was produced"))
})
}
pub(super) fn raw_literal(v: &BoltType, params: &mut Vec<(String, Vec<u8>)>) -> Result<String, DbError> {
    match v {
        BoltType::Bytes(b) => {
            let key = format!("bytes{}", params.len());
            params.push((key.clone(), b.value.to_vec()));
            Ok(format!("${key}"))
        }
        BoltType::Map(map) => {
            let mut fields = map.value.iter().collect::<Vec<_>>();
            fields.sort_by(|(a, _), (b, _)| a.value.cmp(&b.value));
            Ok(format!(
                "{{{}}}",
                fields
                    .into_iter()
                    .map(|(k, v)| Ok(format!("{}:{}", ident(&k.value), raw_literal(v, params)?)))
                    .collect::<Result<Vec<_>, DbError>>()?
                    .join(",")
            ))
        }
        BoltType::List(list) => Ok(format!(
            "[{}]",
            list.iter()
                .map(|v| raw_literal(v, params))
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        )),
        _ => literal(&map_value(v)?),
    }
}
impl NeoSession {
    pub(super) async fn native_backup(&self, cancel: CancelToken) -> Result<BackupData, DbError> {
        let schema = self
            .load_snapshot()
            .await?
            .objects
            .into_iter()
            .filter(|o| ["constraint", "index"].contains(&o.kind.as_str()))
            .map(|o| (o.kind, o.name, o.definition))
            .collect();
        let graph = neo4rs::Graph::connect(self.operation_config(3600000)).map_err(|e| self.error(e))?;
        let mut tx = graph.start_txn().await.map_err(|e| self.error(e))?;
        let result = async {
            let mut clauses = vec![];
            let mut bytes_params = vec![];
            let mut ids = std::collections::HashMap::new();
            let mut size = 0;
            let mut stream = tx
                .execute(query("MATCH (n) RETURN n ORDER BY id(n)"))
                .await
                .map_err(|e| self.error(e))?;
            while let Some(row) = stream.next(&mut tx).await.map_err(|e| self.error(e))? {
                cancelled(&cancel)?;
                let raw: BoltType = row.get("n").map_err(|_| DbError::query("Invalid node"))?;
                let Value::Node(n) = map_value(&raw)? else {
                    return Err(DbError::query("Expected node"));
                };
                let i = ids.len();
                ids.insert(n.id, i);
                let text = format!(
                    "(n{i}{} {})",
                    n.labels
                        .iter()
                        .map(|l| format!(":{}", ident(l)))
                        .collect::<String>(),
                    raw_literal(
                        &match &raw {
                            BoltType::Node(n) => BoltType::Map(n.properties.clone()),
                            _ => unreachable!(),
                        },
                        &mut bytes_params
                    )?
                );
                size += text.len();
                if size > BACKUP_MAX_BYTES {
                    return Err(DbError::query("Backup exceeds 8 MiB"));
                }
                clauses.push(text);
            }
            let node_count = ids.len() as u64;
            let mut rel_count = 0;
            let mut stream = tx
                .execute(query("MATCH ()-[r]->() RETURN r ORDER BY id(r)"))
                .await
                .map_err(|e| self.error(e))?;
            while let Some(row) = stream.next(&mut tx).await.map_err(|e| self.error(e))? {
                cancelled(&cancel)?;
                let raw: BoltType = row
                    .get("r")
                    .map_err(|_| DbError::query("Invalid relationship"))?;
                let Value::Rel(r) = map_value(&raw)? else {
                    return Err(DbError::query("Expected relationship"));
                };
                let a = ids
                    .get(&r.start)
                    .ok_or_else(|| DbError::query("Backup graph changed while reading"))?;
                let b = ids
                    .get(&r.end)
                    .ok_or_else(|| DbError::query("Backup graph changed while reading"))?;
                let text = format!(
                    "(n{a})-[:{} {}]->(n{b})",
                    ident(&r.rel_type),
                    raw_literal(
                        &match &raw {
                            BoltType::Relation(r) => BoltType::Map(r.properties.clone()),
                            _ => unreachable!(),
                        },
                        &mut bytes_params
                    )?
                );
                size += text.len();
                if size > BACKUP_MAX_BYTES {
                    return Err(DbError::query("Backup exceeds 8 MiB"));
                }
                clauses.push(text);
                rel_count += 1;
            }
            let dump = Dump {
                schema,
                bytes_params,
                cypher: if clauses.is_empty() {
                    String::new()
                } else {
                    format!("CREATE {}", clauses.join(",\n"))
                },
            };
            let backup = BackupData {
                format: "neo4j-cypher-v1".into(),
                data: serde_json::to_vec(&dump)
                    .map_err(|_| DbError::query("Cannot encode Cypher backup"))?,
                counts: vec![
                    ("nodes".into(), node_count),
                    ("relationships".into(), rel_count),
                ],
            };
            backup.validate("neo4j-cypher-v1")?;
            Ok(backup)
        }
        .await;
        tx.rollback().await.map_err(|e| self.error(e))?;
        result
    }
    pub(super) async fn native_restore(
        &self,
        backup: &BackupData,
        overwrite: bool,
        cancel: CancelToken,
    ) -> Result<(), DbError> {
        backup.validate("neo4j-cypher-v1")?;
        if overwrite {
            return Err(DbError::unsupported(
                "Neo4j overwrite requires atomic database replacement and is currently disabled",
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
        let snapshot = self.load_snapshot().await?;
        if snapshot.objects.iter().any(|o| {
            o.kind == "constraint" || o.kind == "index" && !o.definition.contains("LOOKUP")
        }) {
            return Err(DbError::new(
                ErrorKind::Conflict,
                "Restore requires an empty schema",
            ));
        }
        let dump: Dump = serde_json::from_slice(&backup.data)
            .map_err(|_| DbError::query("Invalid Cypher backup"))?;
        let mut created = vec![];
        let result = async {
            for (kind, name, sql) in &dump.schema {
                cancelled(&cancel)?;
                if sql.contains("LOOKUP") {
                    continue;
                }
                let req = ExecRequest {
                    text: sql.clone(),
                    read_only: false,
                    params: Default::default(),
                    row_limit: 1,
                    graph_node_limit: 1,
                    timeout_ms: 30000,
                    preview_sample_limit: 1,
                };
                self.bounded(&req, cancel.clone(), false).await?;
                created.push((kind, name));
            }
            if !dump.cypher.is_empty() {
                let graph=neo4rs::Graph::connect(self.operation_config(60000)).map_err(|e|self.error(e))?;
                let mut tx=graph.start_txn().await.map_err(|e|self.error(e))?;
                let mut q=query(&dump.cypher);
                for (key,value) in dump.bytes_params{q=q.param(&key,BoltType::Bytes(neo4rs::BoltBytes::new(value.into())));}
                let result=tokio::select!{r=tx.run(q)=>r.map_err(|e|self.error(e)),_=cancel.cancelled()=>Err(DbError::new(ErrorKind::Cancelled,"Restore cancelled")),_=tokio::time::sleep(Duration::from_secs(60))=>Err(DbError::new(ErrorKind::Timeout,"Restore timed out"))};
                if result.is_ok()&&!cancel.is_cancelled(){tx.commit().await.map_err(|e|{let mut error=self.error(e);error.message=format!("Restore commit: {}",error.message);error})?;}else{let _=tokio::time::timeout(Duration::from_secs(3),tx.rollback()).await;result?;cancelled(&cancel)?;}
            }
            Ok(())
        }
        .await;
        if result.is_err() {
            for (kind, name) in created {
                let req = ExecRequest {
                    text: format!("DROP {} {} IF EXISTS", kind.to_uppercase(), ident(name)),
                    read_only: false,
                    params: Default::default(),
                    row_limit: 1,
                    graph_node_limit: 1,
                    timeout_ms: 30000,
                    preview_sample_limit: 1,
                };
                if self.bounded(&req, CancelToken::new(), false).await.is_err() {
                    return Err(DbError::new(
                        ErrorKind::Connection,
                        "Restore schema cleanup failed",
                    ));
                }
            }
        }
        result
    }
    pub(super) async fn native_roles(&self) -> Result<RolesSnapshot, DbError> {
        let edition = self
            .internal(
                "CALL dbms.components() YIELD edition RETURN edition".into(),
                Default::default(),
            )
            .await?;
        let enterprise = edition.table.is_some_and(|t| {
            t.rows
                .iter()
                .flatten()
                .any(|v| matches!(v,Value::Text(s) if s.eq_ignore_ascii_case("enterprise")))
        });
        if !enterprise {
            return Ok(RolesSnapshot{supported:false,reason:Some("Neo4j Community does not support role and privilege administration; Enterprise is required".into()),..Default::default()});
        }
        let users = self
            .internal(
                "SHOW USERS YIELD user,roles RETURN user,roles".into(),
                Default::default(),
            )
            .await?;
        if users.truncated {
            return Err(DbError::query("User list exceeds limit"));
        }
        let mut snapshot = RolesSnapshot {
            supported: true,
            ..Default::default()
        };
        if let Some(table) = users.table {
            for row in table.rows {
                let name = row
                    .first()
                    .and_then(|v| {
                        if let Value::Text(s) = v {
                            Some(s.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                let member_of = row
                    .get(1)
                    .and_then(|v| {
                        if let Value::List(rs) = v {
                            Some(
                                rs.iter()
                                    .filter_map(|v| {
                                        if let Value::Text(s) = v {
                                            Some(s.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect(),
                            )
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();
                snapshot.roles.push(RoleInfo{name,login:true,superuser:false,member_of,summary:vec!["Role membership; use Enterprise privilege inspection for fine-grained access".into()]});
            }
        }
        Ok(snapshot)
    }
}
pub(super) fn cancelled(c: &CancelToken) -> Result<(), DbError> {
    if c.is_cancelled() {
        Err(DbError::new(ErrorKind::Cancelled, "Operation cancelled"))
    } else {
        Ok(())
    }
}
