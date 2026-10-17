use super::*;
impl PgSession {
    pub(super) async fn edit_run(
        &self,
        req: &EditRequest,
        state: &mut TxState,
        cancel: &CancelToken,
        preview: bool,
    ) -> Result<RunOutput, DbError> {
        if req.edits.len() > 1000 {
            return Err(DbError::unsupported("편집은 최대 1,000건까지 지원합니다"));
        }
        let path = parse_path(&req.node_id)?;
        if path.len() != 4 || path[0] != "table" || path[2] != "tables" {
            return Err(DbError::query("올바르지 않은 테이블 노드입니다"));
        }
        let name = format!("{}.{}", quote_ident(&path[1]), quote_ident(&path[3]));
        let nested = state.explicit;
        let sp = format!("axdb_edit_{}", uuid_free_id());
        let result=async {
        self.client
            .batch_execute(&if nested {
                format!("SAVEPOINT {sp}")
            } else {
                "BEGIN".into()
            })
            .await
            .map_err(|e| self.error(e))?;
            // Resolve only real ordinary/partitioned tables and their real, non-dropped columns.
            let rows=self.client.query("SELECT a.attname, tn.nspname, t.typname, a.atttypid, EXISTS(SELECT 1 FROM pg_index i WHERE i.indrelid=c.oid AND i.indisprimary AND a.attnum IN (SELECT k.attnum FROM unnest(i.indkey) WITH ORDINALITY AS k(attnum,ord) WHERE k.ord<=i.indnkeyatts)) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace JOIN pg_attribute a ON a.attrelid=c.oid JOIN pg_type t ON t.oid=a.atttypid JOIN pg_namespace tn ON tn.oid=t.typnamespace WHERE n.nspname=$1 AND c.relname=$2 AND c.relkind IN ('r','p') AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum",&[&path[1],&path[3]]).await.map_err(|e|self.error(e))?;
            if rows.is_empty(){return Err(DbError::query("알 수 없는 테이블입니다"));}
            let columns=rows.iter().map(|r|r.get::<_,String>(0)).collect::<Vec<_>>();
            let keys=rows.iter().filter(|r|r.get::<_,bool>(4)).map(|r|r.get::<_,String>(0)).collect::<Vec<_>>();
            let plans=dbcore::edits::plan(req,&name,&columns,&keys,quote_ident,|i,n,v|{
                if matches!(v, Value::Bytes { .. }) { return format!("${i}"); }
                let r=&rows[columns.iter().position(|c|c==n).unwrap()];format!("(${i}::text)::{}.{}",quote_ident(r.get(1)),quote_ident(r.get(2)))
            })?;
            let statement_text=plans.iter().map(|p|p.sql.clone()).collect::<Vec<_>>().join(";\n");
            let returning=columns.iter().map(|n|format!("{}::text",quote_ident(n))).collect::<Vec<_>>().join(", ");
            let select=columns.iter().map(|n|format!("{}::text",quote_ident(n))).collect::<Vec<_>>().join(", ");
            let mut changed=EditResult{affected:vec![],statements:vec![]};let mut sample=ChangePreview::default();
            for (index,plan) in plans.into_iter().enumerate(){
                let result=async {
                    if cancel.is_cancelled(){return Err(DbError::new(ErrorKind::Cancelled,"편집이 취소되었습니다"));}
                    let values=plan.values.iter().map(|value| match value {
                        Value::Bytes { len, hex } if len.checked_mul(2) == Some(hex.len() as u64) && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
                            let decoded = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| DbError::query("Invalid bytes"))).collect::<Result<Vec<_>, _>>()?;
                            Ok(Box::new(decoded) as Box<dyn tokio_postgres::types::ToSql + Sync + Send>)
                        }
                        _ => Ok(Box::new(dbcore::edits::text(value)?) as Box<dyn tokio_postgres::types::ToSql + Sync + Send>),
                    }).collect::<Result<Vec<_>, DbError>>()?;
                    let params=values.iter().map(|v|&**v as &(dyn tokio_postgres::types::ToSql+Sync)).collect::<Vec<_>>();
                    let mut statement=StatementPreview::new(plan.sql.clone(),plan.kind.clone());
                    let convert=|output:Vec<tokio_postgres::Row>|->Result<Table,DbError>{
                        Ok(Table{columns:rows.iter().map(|r|Column{name:r.get(0),type_name:r.get(2)}).collect(),rows:output.into_iter().take(req.preview_sample_limit.min(1000) as usize).map(|r|rows.iter().enumerate().map(|(i,c)|map_value(&Type::from_oid(c.get(3)).unwrap_or(Type::TEXT),r.get(i))).collect()).collect::<Result<_,_>>()?})
                    };
                    if preview {if let Some(before)=plan.before {
                        let before=before.replacen("SELECT *",&format!("SELECT {select}"),1);
                        let types=plan.values.iter().map(|v|if matches!(v,Value::Bytes{..}){Type::BYTEA}else{Type::TEXT}).collect::<Vec<_>>();
                        let stmt=self.client.prepare_typed(&before,&types).await.map_err(|e|self.error(e))?;
                        statement.before=Some(convert(self.client.query(&stmt,&params).await.map_err(|e|self.error(e))?)?);
                    }}
                    let sql=format!("{} RETURNING {returning}",plan.sql);
                    let types=plan.values.iter().map(|v|if matches!(v,Value::Bytes{..}){Type::BYTEA}else{Type::TEXT}).collect::<Vec<_>>();
                    let stmt=self.client.prepare_typed(&sql,&types).await.map_err(|e|self.error(e))?;
                    // Drain every row so affected reflects the complete mutation.
                    let stream=self.client.query_raw(&stmt,params.iter().copied()).await.map_err(|e|self.error(e))?;
                    pin_mut!(stream);let mut output=vec![];let mut count=0;
                    while let Some(row)=stream.next().await{let row=row.map_err(|e|self.error(e))?;count+=1;if preview&&output.len()<req.preview_sample_limit.min(1000) as usize{output.push(row);}}
                    if plan.kind!=StatementKind::Insert&&count!=1{return Err(DbError::new(ErrorKind::Conflict,format!("정확히 1개 행이 영향을 받아야 하지만 {count}개 행이 영향을 받았습니다")));}
                    statement.affected=Some(count);if preview&&plan.kind!=StatementKind::Delete{statement.after=Some(convert(output)?);}
                    changed.affected.push(count);changed.statements.push(plan.sql);sample.statements.push(statement);Ok(())
                }.await;
                result.map_err(|mut e:DbError|{e.message=format!("Edit {index}: {}",e.message);e.detail=Some(statement_text.clone());e})?;
            }
            if cancel.is_cancelled(){return Err(DbError::new(ErrorKind::Cancelled,"편집이 취소되었습니다"));}
            if preview {sample.notes.push("모든 편집을 롤백했습니다. 시퀀스와 트리거의 외부 영향은 되돌릴 수 없습니다.".into());Ok(RunOutput::Preview(sample))}else{Ok(RunOutput::Edit(changed))}
        }.await;
        let rollback = preview || result.is_err() || cancel.is_cancelled();
        let cleanup = if nested {
            if rollback {
                format!("ROLLBACK TO SAVEPOINT {sp}; RELEASE SAVEPOINT {sp}")
            } else {
                format!("RELEASE SAVEPOINT {sp}")
            }
        } else if rollback {
            "ROLLBACK".into()
        } else {
            "COMMIT".into()
        };
        if let Err(e) = self.client.batch_execute(&cleanup).await {
            self.connection.abort();
            self.closed.store(true, Ordering::Release);
            return Err(DbError::new(
                ErrorKind::Connection,
                format!(
                    "편집 정리에 실패해 세션을 폐기했습니다: {}",
                    self.error(e).message
                ),
            ));
        }
        result
    }
}
