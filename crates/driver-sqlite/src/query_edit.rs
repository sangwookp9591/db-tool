use super::*;
pub(super) fn target(
    conn: &Connection,
    stmt: &rusqlite::Statement<'_>,
) -> Result<EditTarget, &'static str> {
    let sql = stmt
        .expanded_sql()
        .ok_or("쿼리 출처를 확인할 수 없습니다")?;
    let source = dbcore::query_edit::source(&sql, &sqlparser::dialect::SQLiteDialect {})?;
    if source.len() > 2 || (source.len() == 2 && source[0] != "main") {
        return Err("main 데이터베이스 테이블만 편집할 수 있습니다");
    }
    let table = source.last().ok_or("테이블 출처를 확인할 수 없습니다")?;
    let kind: String = conn
        .query_row(
            "SELECT type FROM main.sqlite_master WHERE name=?1",
            [table],
            |r| r.get(0),
        )
        .map_err(|_| "테이블 출처를 확인할 수 없습니다")?;
    if kind != "table" {
        return Err("뷰 결과는 편집할 수 없습니다");
    }
    let meta = stmt.columns_with_metadata();
    dbcore::query_edit::unique_labels(meta.iter().map(|c| c.name()))?;
    if meta.iter().any(|c| {
        c.table_name().is_some_and(|t| t != table) || c.database_name().is_some_and(|d| d != "main")
    }) {
        return Err("여러 테이블 결과는 편집할 수 없습니다");
    }
    let node = format!("table:{table}");
    let (_, keys) = edits::target(conn, &node).map_err(|_| "기본 키를 확인할 수 없습니다")?;
    let mut catalog = conn
        .prepare("SELECT name FROM pragma_table_xinfo(?1) WHERE hidden=0")
        .map_err(|_| "컬럼을 확인할 수 없습니다")?;
    let writable = catalog
        .query_map([table], |r| r.get::<_, String>(0))
        .map_err(|_| "컬럼을 확인할 수 없습니다")?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "컬럼을 확인할 수 없습니다")?;
    let origins = meta
        .iter()
        .map(|c| {
            c.origin_name()
                .filter(|n| writable.iter().any(|w| w == n))
                .map(str::to_owned)
        })
        .collect();
    dbcore::query_edit::target(node, keys, origins)
}
