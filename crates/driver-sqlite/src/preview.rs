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
        return Ok(ChangePreview { notes: vec!["Preview refused inside a user transaction: SQLite interruption/conflict handling can roll back the outer transaction; finish it first".into()], ..Default::default() });
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
                return Err(DbError::new(ErrorKind::Cancelled, "Preview cancelled"));
            }
            if Instant::now() >= deadline {
                return Err(DbError::new(ErrorKind::Timeout, "Preview timed out"));
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
                        .push("Before sample unavailable for this SQL shape".into());
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
                    .push("After sample uses the query's existing RETURNING projection".into());
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
                    "Preview rollback failed; session discarded",
                )
            })?;
    }
    progress?;
    auth?;
    result?;
    preview.notes.push("All data and DDL changes were rolled back; sqlite_master definitions identify schema changes. Triggers may invoke external functions; locks remain until rollback".into());
    Ok(preview)
}
