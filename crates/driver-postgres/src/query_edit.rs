use super::*;
impl PgSession {
    pub(super) async fn query_edit_target(
        &self,
        sql: &str,
        columns: &[tokio_postgres::Column],
    ) -> Result<EditTarget, &'static str> {
        dbcore::query_edit::unique_labels(columns.iter().map(|c| c.name()))?;
        let source = dbcore::query_edit::source(sql, &sqlparser::dialect::PostgreSqlDialect {})?;
        let oids = columns
            .iter()
            .filter_map(|c| c.table_oid())
            .collect::<std::collections::HashSet<_>>();
        if oids.len() != 1 {
            return Err(if oids.is_empty() {
                "계산 컬럼"
            } else {
                "여러 테이블 결과는 편집할 수 없습니다"
            });
        }
        let oid = *oids.iter().next().unwrap();
        let relation = source
            .iter()
            .map(|s| quote_ident(s))
            .collect::<Vec<_>>()
            .join(".");
        let table = self.client.query_opt("SELECT n.nspname,c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.oid=$1 AND c.oid=to_regclass($2)::oid AND c.relkind IN ('r','p')", &[&oid,&relation]).await.map_err(|_| "테이블 출처를 확인할 수 없습니다")?.ok_or("뷰 또는 원본 테이블이 아닌 결과입니다")?;
        let schema: String = table.get(0);
        let name: String = table.get(1);
        let catalog = self.client.query("SELECT a.attnum,a.attname,EXISTS(SELECT 1 FROM pg_index i WHERE i.indrelid=a.attrelid AND i.indisprimary AND a.attnum IN (SELECT k.attnum FROM unnest(i.indkey) WITH ORDINALITY AS k(attnum,ord) WHERE k.ord<=i.indnkeyatts)) FROM pg_attribute a WHERE a.attrelid=$1 AND a.attnum>0 AND NOT a.attisdropped AND a.attgenerated='' ORDER BY a.attnum", &[&oid]).await.map_err(|_| "기본 키를 확인할 수 없습니다")?;
        let keys = catalog
            .iter()
            .filter(|r| r.get::<_, bool>(2))
            .map(|r| r.get::<_, String>(1))
            .collect();
        let origins = columns
            .iter()
            .map(|c| {
                catalog
                    .iter()
                    .find(|r| {
                        Some(r.get::<_, i16>(0)) == c.column_id() && c.table_oid() == Some(oid)
                    })
                    .map(|r| r.get::<_, String>(1))
            })
            .collect();
        dbcore::query_edit::target(
            serde_json::json!(["table", schema, "tables", name]).to_string(),
            keys,
            origins,
        )
    }
}
