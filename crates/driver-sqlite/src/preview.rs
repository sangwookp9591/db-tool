use super::*;
use dbcore::preview::{before_query, kind, rejection, schema_diff, tokens};
use std::collections::BTreeMap;
pub(super) enum RunOutput {
    Execute(Vec<ResultSet>),
    Edit(EditResult),
    Preview(ChangePreview),
}
fn schema(conn: &Connection) -> Result<BTreeMap<String, String>, DbError> {
    let mut stmt = conn.prepare("SELECT 'main:'||type||':'||name, coalesce(sql,'') FROM sqlite_master UNION ALL SELECT 'temp:'||type||':'||name, coalesce(sql,'') FROM sqlite_temp_master").map_err(sql_error)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(sql_error)?;
    rows.collect::<Result<_, _>>().map_err(sql_error)
}
fn table(conn: &Connection, sql: &str, limit: usize) -> Result<Option<Table>, DbError> {
    let mut stmt = conn.prepare(sql).map_err(sql_error)?;
    if stmt.column_count() == 0 {
        stmt.execute([]).map_err(sql_error)?;
        return Ok(None);
    }
    let mut columns: Vec<Column> = stmt
        .columns()
        .iter()
        .map(|c| Column {
            name: c.name().into(),
            type_name: c.decl_type().unwrap_or("").into(),
        })
        .collect();
    let mut rows = stmt.query([]).map_err(sql_error)?;
    let mut output = Vec::new();
    // Drain RETURNING fully, even when the sample is full, to complete every mutation and count.
    while let Some(row) = rows.next().map_err(sql_error)? {
        if output.len() >= limit {
            continue;
        }
        let mut cells = Vec::new();
        for (i, c) in columns.iter_mut().enumerate() {
            let raw = row.get_ref(i).map_err(sql_error)?;
            if c.type_name.is_empty() {
                c.type_name = storage_type(raw).into();
            }
            cells.push(map_value(raw, &c.type_name)?);
        }
        output.push(cells);
    }
    Ok(Some(Table {
        columns,
        rows: output,
    }))
}
pub(super) fn run(
    conn: &Connection,
    req: &ExecRequest,
    cancel: &CancelToken,
    deadline: Instant,
) -> Result<ChangePreview, DbError> {
    if !conn.is_autocommit() {
        return Ok(ChangePreview { notes: vec!["사용자 트랜잭션 안에서는 미리보기를 거부했습니다: SQLite의 중단·충돌 처리로 바깥 트랜잭션이 롤백될 수 있으니 먼저 트랜잭션을 마치세요.".into()], ..Default::default() });
    }
    // Split with quote/comment awareness before opening the savepoint; SQLite prepares later DDL lazily.
    let ts = tokens(&req.text)?;
    let mut statements = Vec::new();
    let mut start = 0;
    for t in &ts {
        if t.text == ";" && t.depth == 0 {
            statements.push(req.text[start..t.start].to_string());
            start = t.start + 1;
        }
    }
    statements.push(req.text[start..].to_string());
    let mut preview = ChangePreview::default();
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
    conn.execute_batch("SAVEPOINT axdb_change_preview")
        .map_err(sql_error)?;
    let token = cancel.clone();
    let result = (|| {
        conn.authorizer(Some(|ctx: AuthContext<'_>| match ctx.action {
            AuthAction::Pragma { .. }
            | AuthAction::Attach { .. }
            | AuthAction::Detach { .. }
            | AuthAction::Transaction { .. }
            | AuthAction::Savepoint { .. } => Authorization::Deny,
            AuthAction::Function { function_name }
                if ["load_extension", "writefile", "readfile"]
                    .iter()
                    .any(|f| function_name.eq_ignore_ascii_case(f)) =>
            {
                Authorization::Deny
            }
            _ => Authorization::Allow,
        }))
        .map_err(sql_error)?;
        conn.progress_handler(
            1000,
            Some(move || token.is_cancelled() || Instant::now() >= deadline),
        )
        .map_err(sql_error)?;
        let limit = req.preview_sample_limit.min(1000) as usize;
        for sql in statements {
            if cancel.is_cancelled() {
                return Err(DbError::new(ErrorKind::Cancelled, "미리보기가 취소되었습니다"));
            }
            if Instant::now() >= deadline {
                return Err(DbError::new(ErrorKind::Timeout, "미리보기 시간이 초과되었습니다"));
            }
            let ts = tokens(&sql)?;
            if ts.is_empty() {
                continue;
            }
            let mut statement = StatementPreview::new(sql.clone(), kind(&ts));
            let before_schema = if statement.kind == StatementKind::Ddl {
                Some(schema(conn)?)
            } else {
                None
            };
            if matches!(
                statement.kind,
                StatementKind::Update | StatementKind::Delete
            ) {
                if let Some(before) = before_query(&sql, &ts, limit as u32, false) {
                    statement.before = table(conn, &before, limit)?;
                } else {
                    statement
                        .warnings
                        .push("이 SQL 형태에서는 변경 전 샘플을 만들 수 없습니다.".into());
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
            let after = table(conn, &executable, limit)?;
            statement.affected = if dml { Some(conn.changes()) } else { None };
            if statement.kind != StatementKind::Delete {
                statement.after = after;
            }
            if has_returning {
                statement
                    .warnings
                    .push("변경 후 샘플은 쿼리의 기존 RETURNING 결과를 사용합니다.".into());
            }
            if let Some(before) = before_schema {
                statement.schema_diff = Some(schema_diff(before, schema(conn)?));
            }
            preview.statements.push(statement);
        }
        Ok(())
    })();
    // Disable cancellation hooks before cleanup so cancellation cannot leave a savepoint behind.
    let progress = conn
        .progress_handler(0, None::<fn() -> bool>)
        .map_err(sql_error);
    let auth = conn
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
        .map_err(sql_error);
    if !conn.is_autocommit() {
        conn.execute_batch("ROLLBACK TO axdb_change_preview; RELEASE axdb_change_preview")
            .map_err(|_| {
                DbError::new(
                    ErrorKind::Connection,
                    "미리보기 롤백에 실패해 세션을 폐기했습니다",
                )
            })?;
    }
    progress?;
    auth?;
    result?;
    preview.notes.push("모든 데이터·DDL 변경을 롤백했습니다. 스키마 변경은 sqlite_master 정의로 확인하세요. 트리거가 외부 함수를 호출할 수 있고, 롤백 전까지 잠금이 유지됩니다.".into());
    Ok(preview)
}
