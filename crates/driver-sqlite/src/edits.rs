use super::*;
use dbcore::edits::Plan;
pub(super) fn target(conn: &Connection, node: &str) -> Result<(Vec<String>, Vec<String>), DbError> {
    let name = node
        .strip_prefix("table:")
        .ok_or_else(|| DbError::query("올바르지 않은 테이블 노드입니다"))?;
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
    stop_interrupts: impl FnOnce(),
) -> Result<RunOutput, DbError> {
    if req.edits.len() > dbcore_limit(req) {
        return Err(DbError::unsupported("편집/가져오기 행 수 상한을 초과했습니다"));
    }
    if !conn.is_autocommit() {
        return Err(DbError::unsupported(
            "편집하기 전에 사용자 트랜잭션을 마치세요",
        ));
    }
    conn.execute_batch("SAVEPOINT axdb_row_edit")
        .map_err(sql_error)?;
    let token = cancel.clone();
    let result = (|| {
        conn.progress_handler(
            1000,
            Some(move || token.is_cancelled() || Instant::now() >= deadline),
        )
        .map_err(sql_error)?;
        let name = req.node_id.strip_prefix("table:").ok_or_else(|| DbError::query("Select a table node"))?;
        let create_sql = dbcore::import_create_sql(req, &format!("main.{}", quote(name)), "sqlite")?;
        if let Some(sql) = &create_sql {
            conn.execute_batch(sql).map_err(sql_error)?;
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
        let mut changed = EditResult {
            affected: vec![],
            statements: vec![],
        };
        let mut sample = ChangePreview::default();
        if let Some(sql) = create_sql { changed.affected.push(0); changed.statements.push(sql.clone()); if preview { sample.statements.push(StatementPreview::new(sql, StatementKind::Ddl)); } }
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
                    return Err(DbError::new(ErrorKind::Cancelled, "편집이 취소되었습니다"));
                }
                if Instant::now() >= deadline {
                    return Err(DbError::new(ErrorKind::Timeout, "편집 시간이 초과되었습니다"));
                }
                let values = values.iter().map(value).collect::<Result<Vec<_>, _>>()?;
                let mut statement = StatementPreview::new(sql.clone(), kind.clone());
                if preview && let Some(before) = before {
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
                let (after, _) = table(
                    conn,
                    &format!("{sql} RETURNING *"),
                    &values,
                    if preview && (req.edits.len() <= 1000 || index < req.preview_sample_limit.min(1000) as usize) {
                        req.preview_sample_limit.min(1000) as usize
                    } else {
                        0
                    },
                )?;
                let count = conn.changes();
                if kind != StatementKind::Insert && count != 1 {
                    return Err(DbError::new(
                        ErrorKind::Conflict,
                        format!("정확히 1개 행이 영향을 받아야 하지만 {count}개 행이 영향을 받았습니다"),
                    ));
                }
                statement.affected = Some(count);
                if kind != StatementKind::Delete {
                    statement.after = Some(after);
                }
                changed.affected.push(count);
                changed.statements.push(sql);
                if req.edits.len() <= 1000 || index < req.preview_sample_limit.min(1000) as usize { sample.statements.push(statement); }
                Ok(())
            })();
            result.map_err(|mut e: DbError| {
                e.message = format!("Edit {index}: {}", e.message);
                e.detail = Some(statement_text.clone());
                e
            })?;
        }
        if cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "편집이 취소되었습니다"));
        }
        if Instant::now() >= deadline {
            return Err(DbError::new(ErrorKind::Timeout, "편집 시간이 초과되었습니다"));
        }
        if preview {
            sample
                .notes
                .push("모든 편집을 롤백했습니다. 트리거가 외부에 영향을 줄 수 있습니다.".into());
            Ok(RunOutput::Preview(sample))
        } else {
            Ok(RunOutput::Edit(changed))
        }
    })();
    stop_interrupts();
    let hooks = conn.progress_handler(0, None::<fn() -> bool>);
    let cleanup = if result.is_err() || preview || hooks.is_err() {
        "ROLLBACK TO axdb_row_edit; RELEASE axdb_row_edit"
    } else {
        "RELEASE axdb_row_edit"
    };
    if !conn.is_autocommit() && conn.execute_batch(cleanup).is_err() {
        return Err(DbError::new(
            ErrorKind::Connection,
            "편집 정리에 실패해 세션을 폐기했습니다",
        ));
    }
    hooks.map_err(|_| DbError::new(ErrorKind::Connection, "편집 훅 정리에 실패했습니다"))?;
    result
}

fn dbcore_limit(req: &EditRequest) -> usize { if req.edits.iter().all(|e|matches!(e,RowEdit::Insert{..})) {10_000} else {1000} }
