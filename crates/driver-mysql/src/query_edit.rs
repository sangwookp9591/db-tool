use super::*;
pub(super) async fn target(conn: &mut Conn, sql: &str) -> Result<EditTarget, &'static str> {
    let source = dbcore::query_edit::source(sql, &MySqlDialect {})?;
    let statement = conn
        .prep(sql)
        .await
        .map_err(|_| "컬럼 출처를 확인할 수 없습니다")?;
    let columns = statement.columns();
    let labels = columns
        .iter()
        .map(|c| c.name_str().into_owned())
        .collect::<Vec<_>>();
    dbcore::query_edit::unique_labels(labels.iter().map(String::as_str))?;
    let tables = columns
        .iter()
        .filter(|c| !c.org_table_str().is_empty())
        .map(|c| (c.schema_str().into_owned(), c.org_table_str().into_owned()))
        .collect::<std::collections::HashSet<_>>();
    if tables.len() != 1 {
        return Err(if tables.is_empty() {
            "계산 컬럼"
        } else {
            "여러 테이블 결과는 편집할 수 없습니다"
        });
    }
    let Some((db, table)) = tables.into_iter().next() else {
        return Err("컬럼 출처를 확인할 수 없습니다");
    };
    if source.last() != Some(&table) || source.len() > 2 || (source.len() == 2 && source[0] != db) {
        return Err("뷰 또는 원본 테이블이 아닌 결과입니다");
    }
    let kind:Option<String>=conn.exec_first("SELECT TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA=? AND TABLE_NAME=?",(&db,&table)).await.map_err(|_|"테이블 출처를 확인할 수 없습니다")?;
    if kind.as_deref() != Some("BASE TABLE") {
        return Err("뷰 결과는 편집할 수 없습니다");
    }
    let keys:Vec<String>=conn.exec("SELECT COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA=? AND TABLE_NAME=? AND CONSTRAINT_NAME='PRIMARY' ORDER BY ORDINAL_POSITION",(&db,&table)).await.map_err(|_|"기본 키를 확인할 수 없습니다")?;
    let writable:Vec<String>=conn.exec("SELECT COLUMN_NAME FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=? AND TABLE_NAME=? AND (GENERATION_EXPRESSION IS NULL OR GENERATION_EXPRESSION='')",(&db,&table)).await.map_err(|_|"컬럼을 확인할 수 없습니다")?;
    let origins = columns
        .iter()
        .map(|c| {
            let name = c.org_name_str().into_owned();
            (!name.is_empty() && writable.contains(&name)).then_some(name)
        })
        .collect();
    dbcore::query_edit::target(node_id(&["table", &db, &table]), keys, origins)
}
