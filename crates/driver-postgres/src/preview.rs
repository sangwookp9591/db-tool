use super::*;
use dbcore::preview::{before_query, kind, rejection, schema_diff, tokens};
use std::collections::BTreeMap;

pub(super) enum RunOutput {
    Execute(Vec<ResultSet>),
    Preview(ChangePreview),
}
impl PgSession {
    async fn preview_table(
        &self,
        sql: &str,
        limit: usize,
    ) -> Result<(Option<Table>, u64), DbError> {
        let prepared = self.client.prepare(sql).await.map_err(|e| self.error(e))?;
        let columns = prepared
            .columns()
            .iter()
            .map(|c| Column {
                name: c.name().into(),
                type_name: c.type_().name().into(),
            })
            .collect();
        let stream = self
            .client
            .simple_query_raw(sql)
            .await
            .map_err(|e| self.error(e))?;
        pin_mut!(stream);
        let mut rows = Vec::new();
        let mut affected = 0;
        while let Some(message) = stream.next().await {
            match message.map_err(|e| self.error(e))? {
                SimpleQueryMessage::Row(row) if rows.len() < limit => {
                    rows.push(
                        prepared
                            .columns()
                            .iter()
                            .enumerate()
                            .map(|(i, c)| map_value(c.type_(), row.get(i)))
                            .collect::<Result<Vec<_>, _>>()?,
                    );
                }
                SimpleQueryMessage::CommandComplete(n) => affected = n,
                _ => {}
            }
        }
        Ok((
            if prepared.columns().is_empty() {
                None
            } else {
                Some(Table { columns, rows })
            },
            affected,
        ))
    }
    async fn preview_schema(&self) -> Result<BTreeMap<String, String>, DbError> {
        // Catalog identities include columns and indexes; definitions detect ALTER changes too.
        // Other sessions' temp schemas are excluded: they are not this change and race with their own DROPs.
        const SNAPSHOT: &str = "SELECT 'table:'||quote_ident(n.nspname)||'.'||quote_ident(c.relname), c.relkind::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind IN ('r','p','v','m','f') AND n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname !~ '^pg_toast' AND NOT pg_catalog.pg_is_other_temp_schema(n.oid) UNION ALL SELECT 'column:'||quote_ident(table_schema)||'.'||quote_ident(table_name)||'.'||quote_ident(column_name), concat_ws('|',data_type,udt_schema,udt_name,is_nullable,column_default,character_maximum_length::text,numeric_precision::text,numeric_scale::text) FROM information_schema.columns WHERE table_schema NOT IN ('pg_catalog','information_schema') UNION ALL SELECT 'index:'||quote_ident(n.nspname)||'.'||quote_ident(c.relname), pg_catalog.pg_get_indexdef(c.oid) FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class c ON c.oid=i.indexrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname !~ '^pg_toast' AND NOT pg_catalog.pg_is_other_temp_schema(n.oid)";
        // Concurrent DDL by other sessions can still drop a relation mid-scan ("could not open relation with OID",
        // measured ~3% under a DDL loop). Retry inside a savepoint so the preview transaction stays usable.
        let mut attempt = 0;
        let rows = loop {
            attempt += 1;
            self.client.batch_execute("SAVEPOINT axdb_preview_snapshot").await.map_err(|e| self.error(e))?;
            match self.client.simple_query(SNAPSHOT).await {
                Ok(rows) => {
                    self.client.batch_execute("RELEASE SAVEPOINT axdb_preview_snapshot").await.map_err(|e| self.error(e))?;
                    break rows;
                }
                Err(e) => {
                    self.client.batch_execute("ROLLBACK TO SAVEPOINT axdb_preview_snapshot; RELEASE SAVEPOINT axdb_preview_snapshot").await.map_err(|e| self.error(e))?;
                    let transient = matches!(e.code().map(|c| c.code()), Some("XX000" | "42P01" | "40001"));
                    if !transient || attempt == 3 {
                        return Err(self.error(e));
                    }
                }
            }
        };
        Ok(rows
            .into_iter()
            .filter_map(|r| match r {
                SimpleQueryMessage::Row(r) => Some((r.get(0)?.to_string(), r.get(1)?.to_string())),
                _ => None,
            })
            .collect())
    }
    pub(super) async fn preview_run(
        &self,
        req: &ExecRequest,
        state: &mut TxState,
        cancel: &CancelToken,
    ) -> Result<ChangePreview, DbError> {
        if !req.params.is_empty() {
            return Err(DbError::unsupported(
                "PostgreSQL parameters are not enabled",
            ));
        }
        let statements = split_statements(&req.text)?;
        let mut preview = ChangePreview::default();
        // Preflight the complete batch before executing its first statement.
        for sql in &statements {
            if let Some(reason) = rejection(&tokens(sql)?) {
                preview.notes.push(reason.clone());
                let mut statement = StatementPreview::new(sql.clone(), kind(&tokens(sql)?));
                statement.warnings.push(reason);
                preview.statements.push(statement);
            }
        }
        if !preview.notes.is_empty() {
            return Ok(preview);
        }
        let nested = state.explicit;
        let savepoint = format!("axdb_preview_{}", uuid_free_id());
        let begin = if nested {
            format!("SAVEPOINT {savepoint}")
        } else {
            "BEGIN".into()
        };
        let cleanup = if nested {
            format!("ROLLBACK TO SAVEPOINT {savepoint}; RELEASE SAVEPOINT {savepoint}")
        } else {
            "ROLLBACK".into()
        };
        let result = async {
            self.client
                .batch_execute(&begin)
                .await
                .map_err(|e| self.error(e))?;
            let limit = req.preview_sample_limit.min(1000) as usize;
            for sql in statements {
                if cancel.is_cancelled() {
                    return Err(DbError::new(ErrorKind::Cancelled, "Preview cancelled"));
                }
                let ts = tokens(&sql)?;
                let mut statement = StatementPreview::new(sql.clone(), kind(&ts));
                let schema = if statement.kind == StatementKind::Ddl {
                    Some(self.preview_schema().await?)
                } else {
                    None
                };
                if matches!(
                    statement.kind,
                    StatementKind::Update | StatementKind::Delete
                ) {
                    if let Some(before) = before_query(&sql, &ts, limit as u32, true) {
                        statement.before = self.preview_table(&before, limit).await?.0;
                    } else {
                        statement.warnings.push(
                            "Before sample unavailable for this SQL shape (joins, aliases or CTEs)"
                                .into(),
                        );
                    }
                }
                let dml = matches!(
                    statement.kind,
                    StatementKind::Insert | StatementKind::Update | StatementKind::Delete
                );
                let has_returning = ts.iter().any(|t| t.depth == 0 && t.is("RETURNING"));
                let executable = if dml && !has_returning {
                    format!("{sql}\nRETURNING *")
                } else {
                    sql.clone()
                };
                let (table, affected) = self.preview_table(&executable, limit).await?;
                statement.affected = Some(affected);
                if statement.kind != StatementKind::Delete {
                    statement.after = table;
                }
                if has_returning {
                    statement
                        .warnings
                        .push("After sample uses the query's existing RETURNING projection".into());
                }
                if let Some(before) = schema {
                    statement.schema_diff = Some(schema_diff(before, self.preview_schema().await?));
                }
                preview.statements.push(statement);
            }
            Ok(())
        }
        .await;
        // This runs after success, query errors and cancellation, while retaining the session lock.
        let rolled_back = self
            .client
            .batch_execute(&cleanup)
            .await
            .map_err(|e| self.error(e));
        if rolled_back.is_err() {
            self.connection.abort();
            self.closed.store(true, Ordering::Release);
        }
        rolled_back?;
        result?;
        preview.notes = vec!["All data and DDL changes were rolled back".into(), "Sequence values are not rolled back; triggers/functions may have external side effects. Preview holds locks until rollback and samples are not an apply guarantee".into()];
        Ok(preview)
    }
}
