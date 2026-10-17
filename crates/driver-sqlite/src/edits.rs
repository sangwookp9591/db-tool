use super::*;
use dbcore::edits::Plan;
pub(super) fn target(conn: &Connection, node: &str) -> Result<(Vec<String>, Vec<String>), DbError> {
    let name = node
        .strip_prefix("table:")
        .ok_or_else(|| DbError::query("Invalid table node"))?;
    let kind: String = conn
        .query_row(
            "SELECT type FROM sqlite_master WHERE name=?1 AND type IN ('table','view')",
            [name],
            |r| r.get(0),
        )
        .map_err(sql_error)?;
    let mut stmt = conn
        .prepare("SELECT name,pk FROM pragma_table_info(?1) ORDER BY pk,cid")
        .map_err(sql_error)?;
    let rows = stmt
        .query_map([name], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(sql_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_error)?;
    Ok((
        rows.iter().map(|(n, _)| n.clone()).collect(),
        rows.iter()
            .filter(|(_, pk)| *pk > 0 && kind == "table")
            .map(|(n, _)| n.clone())
            .collect(),
    ))
}
fn value(v: &Value) -> Result<rusqlite::types::Value, DbError> {
    use rusqlite::types::Value as V;
    Ok(match v {
        Value::Null => V::Null,
        Value::Bool(b) => V::Integer(i64::from(*b)),
        Value::Int(n) => V::Integer(n.parse().map_err(|_| DbError::query("Invalid integer"))?),
        Value::Float(FloatValue::Number(n)) if n.is_finite() => V::Real(*n),
        Value::Bytes { len, hex } if len.checked_mul(2) == Some(hex.len() as u64) => V::Blob(
            (0..hex.len())
                .step_by(2)
                .map(|i| {
                    hex.get(i..i + 2)
                        .and_then(|s| u8::from_str_radix(s, 16).ok())
                        .ok_or_else(|| DbError::query("Invalid bytes"))
                })
                .collect::<Result<_, _>>()?,
        ),
        _ => V::Text(dbcore::edits::text(v)?.ok_or_else(|| DbError::query("Invalid value"))?),
    })
}
fn table(
    conn: &Connection,
    sql: &str,
    values: &[rusqlite::types::Value],
    limit: usize,
) -> Result<(Table, u64), DbError> {
    let mut stmt = conn.prepare(sql).map_err(sql_error)?;
    let columns = stmt
        .columns()
        .iter()
        .map(|c| Column {
            name: c.name().into(),
            type_name: c.decl_type().unwrap_or("").into(),
        })
        .collect::<Vec<_>>();
    let mut rows = stmt
        .query(rusqlite::params_from_iter(values))
        .map_err(sql_error)?;
    let mut output = Vec::new();
    let mut count = 0;
    while let Some(row) = rows.next().map_err(sql_error)? {
        count += 1;
        if output.len() < limit {
            output.push(
                columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| map_value(row.get_ref(i).map_err(sql_error)?, &c.type_name))
                    .collect::<Result<_, _>>()?,
            );
        }
    }
    Ok((
        Table {
            columns,
            rows: output,
        },
        count,
    ))
}
pub(super) fn run(
    conn: &Connection,
    req: &EditRequest,
    cancel: &CancelToken,
    deadline: Instant,
    preview: bool,
) -> Result<RunOutput, DbError> {
    if req.edits.len() > 1000 {
        return Err(DbError::unsupported("At most 1000 edits are supported"));
    }
    if !conn.is_autocommit() {
        return Err(DbError::unsupported(
            "Finish the user transaction before editing",
        ));
    }
    let (columns, keys) = target(conn, &req.node_id)?;
    let plans = dbcore::edits::plan(
        req,
        &format!(
            "main.{}",
            quote(req.node_id.strip_prefix("table:").unwrap())
        ),
        &columns,
        &keys,
        quote,
        |i, _, _| format!("?{i}"),
    )?;
    let statement_text = plans
        .iter()
        .map(|p| p.sql.clone())
        .collect::<Vec<_>>()
        .join(";\n");
    conn.execute_batch("SAVEPOINT axdb_row_edit")
        .map_err(sql_error)?;
    let token = cancel.clone();
    let result = (|| {
        conn.progress_handler(
            1000,
            Some(move || token.is_cancelled() || Instant::now() >= deadline),
        )
        .map_err(sql_error)?;
        let mut changed = EditResult {
            affected: vec![],
            statements: vec![],
        };
        let mut sample = ChangePreview::default();
        for (
            index,
            Plan {
                sql,
                before,
                values,
                kind,
            },
        ) in plans.into_iter().enumerate()
        {
            let result = (|| {
                if cancel.is_cancelled() {
                    return Err(DbError::new(ErrorKind::Cancelled, "Editing cancelled"));
                }
                if Instant::now() >= deadline {
                    return Err(DbError::new(ErrorKind::Timeout, "Editing timed out"));
                }
                let values = values.iter().map(value).collect::<Result<Vec<_>, _>>()?;
                let mut statement = StatementPreview::new(sql.clone(), kind.clone());
                if preview {
                    if let Some(before) = before {
                        statement.before = Some(
                            table(
                                conn,
                                &before,
                                &values,
                                req.preview_sample_limit.min(1000) as usize,
                            )?
                            .0,
                        );
                    }
                }
                let (after, _) = table(
                    conn,
                    &format!("{sql} RETURNING *"),
                    &values,
                    if preview {
                        req.preview_sample_limit.min(1000) as usize
                    } else {
                        0
                    },
                )?;
                let count = conn.changes();
                if kind != StatementKind::Insert && count != 1 {
                    return Err(DbError::new(
                        ErrorKind::Conflict,
                        format!("Expected exactly 1 row, affected {count}"),
                    ));
                }
                statement.affected = Some(count);
                if kind != StatementKind::Delete {
                    statement.after = Some(after);
                }
                changed.affected.push(count);
                changed.statements.push(sql);
                sample.statements.push(statement);
                Ok(())
            })();
            result.map_err(|mut e: DbError| {
                e.message = format!("Edit {index}: {}", e.message);
                e.detail = Some(statement_text.clone());
                e
            })?;
        }
        if cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "Editing cancelled"));
        }
        if Instant::now() >= deadline {
            return Err(DbError::new(ErrorKind::Timeout, "Editing timed out"));
        }
        if preview {
            sample
                .notes
                .push("All edits rolled back; triggers may have external side effects".into());
            Ok(RunOutput::Preview(sample))
        } else {
            Ok(RunOutput::Edit(changed))
        }
    })();
    let hooks = conn.progress_handler(0, None::<fn() -> bool>);
    let cleanup = if result.is_err() || preview || hooks.is_err() {
        "ROLLBACK TO axdb_row_edit; RELEASE axdb_row_edit"
    } else {
        "RELEASE axdb_row_edit"
    };
    if !conn.is_autocommit() && conn.execute_batch(cleanup).is_err() {
        return Err(DbError::new(
            ErrorKind::Connection,
            "Edit cleanup failed; session discarded",
        ));
    }
    hooks.map_err(|_| DbError::new(ErrorKind::Connection, "Edit hooks cleanup failed"))?;
    result
}
