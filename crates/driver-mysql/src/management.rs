use super::*;
use crate::catalog::columns;
use sqlparser::ast::{
    visit_expressions, visit_expressions_mut, visit_relations, BinaryOperator, Expr,
};
use std::{collections::BTreeSet, ops::ControlFlow};
fn edit_sql(sql: &str) -> Result<String, DbError> {
    let mut ast = parsed(sql)?;
    if ast.len() != 1 {
        return Err(DbError::query("Expected one generated edit statement"));
    }
    let _: ControlFlow<()> = visit_expressions_mut(&mut ast, |expr| {
        if let Expr::IsNotDistinctFrom(left, right) = expr {
            *expr = Expr::BinaryOp {
                left: left.clone(),
                op: BinaryOperator::Spaceship,
                right: right.clone(),
            };
        }
        ControlFlow::Continue(())
    });
    Ok(ast.remove(0).to_string())
}
fn kind(s: &Statement) -> StatementKind {
    match s {
        Statement::Insert(_) => StatementKind::Insert,
        Statement::Update(_) => StatementKind::Update,
        Statement::Delete(_) => StatementKind::Delete,
        _ => StatementKind::Other,
    }
}
pub(super) fn allowed(ast: &[Statement]) -> Result<(), DbError> {
    if ast.is_empty()
        || ast.iter().any(|s| {
            !matches!(
                s,
                Statement::Query(_)
                    | Statement::Insert(_)
                    | Statement::Update(_)
                    | Statement::Delete(_)
            )
        })
    {
        return Err(DbError::unsupported(
            "MySQL DDL implicitly commits; preview accepts SELECT/INSERT/UPDATE/DELETE only",
        ));
    }
    let mut invalid = false;
    let _: ControlFlow<()> = visit_expressions(&ast.to_vec(), |e| {
        if let Expr::Function(f) = e {
            let name = f.name.to_string().to_ascii_uppercase();
            if ![
                "ABS",
                "AVG",
                "CAST",
                "CEIL",
                "CEILING",
                "COALESCE",
                "CONCAT",
                "CONCAT_WS",
                "COUNT",
                "CURRENT_DATE",
                "CURRENT_TIMESTAMP",
                "DATE",
                "DATE_ADD",
                "DATE_SUB",
                "DATEDIFF",
                "FLOOR",
                "GREATEST",
                "HEX",
                "IF",
                "IFNULL",
                "JSON_ARRAY",
                "JSON_EXTRACT",
                "JSON_OBJECT",
                "JSON_UNQUOTE",
                "LENGTH",
                "LEAST",
                "LOWER",
                "MAX",
                "MIN",
                "MOD",
                "NOW",
                "NULLIF",
                "RAND",
                "REPLACE",
                "ROUND",
                "SLEEP",
                "SUBSTR",
                "SUBSTRING",
                "SUM",
                "TRIM",
                "UNHEX",
                "UPPER",
                "UTC_TIMESTAMP",
                "VERSION",
                "DATABASE",
            ]
            .contains(&name.as_str())
            {
                invalid = true;
            }
        }
        ControlFlow::Continue(())
    });
    if invalid {
        return Err(DbError::unsupported(
            "Protected operations refuse stored/UDF or unrecognized functions",
        ));
    }
    let text = ast
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(";")
        .to_ascii_uppercase();
    let tokens = sqlparser::tokenizer::Tokenizer::new(&MySqlDialect {}, &text)
        .tokenize()
        .map_err(|_| DbError::query("Invalid SQL tokens"))?;
    if tokens.iter().any(|t|matches!(t,sqlparser::tokenizer::Token::Word(w) if w.quote_style.is_none() && matches!(w.value.as_str(),"INTO"|"OUTFILE"|"DUMPFILE"|"PROCEDURE"))){
        // INSERT INTO is safe; SELECT INTO writes files or variables outside rollback semantics.
        if ast.iter().any(|s|matches!(s,Statement::Query(_))) || tokens.iter().any(|t|matches!(t,sqlparser::tokenizer::Token::Word(w) if w.quote_style.is_none() && matches!(w.value.as_str(),"OUTFILE"|"DUMPFILE"|"PROCEDURE"))){return Err(DbError::unsupported("SELECT INTO and external effects cannot be previewed"));}
    }
    Ok(())
}
pub(super) async fn visible_privilege(
    conn: &mut Conn,
    db: &str,
    table: Option<&str>,
    privilege: &str,
) -> Result<(), DbError> {
    let grants: Vec<String> = conn.query("SHOW GRANTS").await.map_err(error)?;
    let allowed = grants.iter().any(|grant| {
        let Some((rights, tail)) = grant
            .strip_prefix("GRANT ")
            .and_then(|g| g.split_once(" ON "))
        else {
            return false;
        };
        let Some((scope, _)) = tail.split_once(" TO ") else {
            return false;
        };
        let has = |privilege: &str| {
            rights == "ALL PRIVILEGES" || rights.split(',').any(|p| p.trim() == privilege)
        };
        let right = if privilege == "backup metadata" {
            ["SELECT", "TRIGGER", "EVENT", "SHOW VIEW", "EXECUTE"]
                .iter()
                .all(|p| has(p))
        } else {
            has(privilege)
        };
        right
            && (scope == "*.*"
                || scope == format!("{}.*", quote(db))
                || table.is_some_and(|t| scope == qualified(db, t)))
    });
    if !allowed {
        return Err(DbError::unsupported(format!("Protected operation requires visible {privilege} privilege for this database/table; role-derived visibility is not assumed")));
    }
    Ok(())
}
async fn transactional_tables(
    conn: &mut Conn,
    ast: &[Statement],
    writes: bool,
) -> Result<(), DbError> {
    let current = conn
        .query_first::<Option<String>, _>("SELECT DATABASE()")
        .await
        .map_err(error)?
        .flatten()
        .unwrap_or_default();
    let mut tables = BTreeSet::new();
    let _: ControlFlow<()> = visit_relations(&ast.to_vec(), |name| {
        let names = name
            .0
            .iter()
            .map(|p| p.as_ident().map(|i| i.value.clone()).unwrap_or_default())
            .collect::<Vec<_>>();
        let (db, table) = match names.as_slice() {
            [table] => (current.clone(), table.clone()),
            [db, table] => (db.clone(), table.clone()),
            _ => (String::new(), String::new()),
        };
        tables.insert((db, table));
        ControlFlow::Continue(())
    });
    for (db, table) in tables {
        if db.is_empty() {
            return Err(DbError::query(
                "Select a database before a protected table operation",
            ));
        }
        if writes {
            visible_privilege(conn, &db, Some(&table), "TRIGGER").await?;
        }
        conn.query_drop(format!("SELECT 1 FROM {} LIMIT 0", qualified(&db, &table)))
            .await
            .map_err(error)?;
        let engine:Option<String>=conn.exec_first("SELECT ENGINE FROM information_schema.TABLES WHERE TABLE_SCHEMA=? AND TABLE_NAME=?",(&db,&table)).await.map_err(error)?.flatten();
        let triggers:Option<u64>=conn.exec_first("SELECT COUNT(*) FROM information_schema.TRIGGERS WHERE EVENT_OBJECT_SCHEMA=? AND EVENT_OBJECT_TABLE=?",(&db,&table)).await.map_err(error)?;
        if engine.as_deref() != Some("InnoDB") || triggers != Some(0) {
            return Err(DbError::unsupported(
                "Rollback preview/edit requires visible InnoDB base tables without triggers",
            ));
        }
    }
    Ok(())
}
impl MysqlSession {
    pub(super) async fn preview_sql(
        &self,
        req: &ExecRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        let ast = parsed(&req.text)?;
        if let Err(e) = allowed(&ast) {
            return Ok(ChangePreview {
                notes: vec![e.message],
                ..Default::default()
            });
        }
        let mut gate = tokio::select! {s=self.conn.lock()=>s,_=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Preview cancelled"))};
        let mut conn = self.operation_connection(&mut gate).await?;
        self.set_timeout(&mut conn, req.timeout_ms).await?;
        conn.query_drop("START TRANSACTION").await.map_err(error)?;
        let id = conn.id();
        let work = async {
            transactional_tables(
                &mut conn,
                &ast,
                ast.iter().any(|s| !matches!(s, Statement::Query(_))),
            )
            .await?;
            let mut statements = vec![];
            for s in &ast {
                let mut r = req.clone();
                r.text = s.to_string();
                r.row_limit = req.preview_sample_limit.clamp(1, 1000);
                let results = query(&mut conn, &r, true).await?;
                for result in results {
                    let mut p = StatementPreview::new(r.text.clone(), kind(s));
                    p.affected = result.affected;
                    p.after = result.table;
                    if p.after.is_none() {
                        p.warnings.push("MySQL does not return changed rows; affected count is available without a row sample".into());
                    }
                    statements.push(p);
                }
            }
            Ok(ChangePreview{statements,committed:false,notes:vec!["Rolled back in an isolated UTC connection; temporary tables are not used and open user transactions are refused.".into(),"AUTO_INCREMENT gaps and lock/function effects are not rolled back; stored/UDF functions, triggers and non-InnoDB tables are refused.".into()]})
        };
        let r = self.guarded(id, cancel, req.timeout_ms, work).await;
        self.cleanup(&mut conn, "ROLLBACK").await?;
        r
    }
    async fn edits(
        &self,
        req: &EditRequest,
        preview: bool,
        cancel: CancelToken,
    ) -> Result<(EditResult, ChangePreview), DbError> {
        if req.create_table.is_some() {
            return Err(DbError::unsupported("MySQL CREATE TABLE implicitly commits; create the table separately before importing"));
        }
        let (db, table) = table_path(&req.node_id)?;
        if req.edits.is_empty() {
            return Err(DbError::query("No edits supplied"));
        }
        let mut gate = tokio::select! {s=self.conn.lock()=>s,_=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Edits cancelled"))};
        let mut conn = self.operation_connection(&mut gate).await?;
        self.set_timeout(&mut conn, req.timeout_ms).await?;
        conn.query_drop("START TRANSACTION").await.map_err(error)?;
        let id = conn.id();
        let work = async {
            transactional_tables(
                &mut conn,
                &parsed(&format!("SELECT * FROM {}", qualified(&db, &table)))?,
                true,
            )
            .await?;
            let cols = columns(&mut conn, &db, &table, self.maria).await?;
            let names = cols
                .iter()
                .filter(|c| !c.generated)
                .map(|c| c.name.clone())
                .collect::<Vec<_>>();
            let keys = cols
                .iter()
                .filter(|c| c.pk)
                .map(|c| c.name.clone())
                .collect::<Vec<_>>();
            let plans = dbcore::edits::plan(
                req,
                &qualified(&db, &table),
                &names,
                &keys,
                quote,
                |_, _, _| "?".into(),
            )?;
            let mut changed = EditResult {
                affected: vec![],
                statements: vec![],
            };
            let mut samples = vec![];
            // Inserts with identical column order share one prepared multi-row INSERT (up to 500 rows).
            let mut index = 0;
            while index < plans.len() {
                if cancel.is_cancelled() {
                    return Err(DbError::new(ErrorKind::Cancelled, "Edits cancelled"));
                }
                let plan = &plans[index];
                let sql = if matches!(plan.kind, StatementKind::Insert) && plan.values.is_empty() {
                    format!("INSERT INTO {} () VALUES ()", qualified(&db, &table))
                } else {
                    edit_sql(&plan.sql)?
                };
                let mut p = StatementPreview::new(sql.clone(), plan.kind.clone());
                if preview && let Some(before) = &plan.before {
                    let key_count = match &req.edits[index] {
                        RowEdit::Update { key, .. } | RowEdit::Delete { key } => key.len() + req.originals.get(index).map_or(0, Vec::len),
                        _ => 0,
                    };
                    let params = plan.values[plan.values.len() - key_count..]
                        .iter()
                        .map(value::bound)
                        .collect::<Result<Vec<_>, _>>()?;
                    p.before = collect(
                        conn.exec_iter(
                            format!(
                                "{} LIMIT {}",
                                edit_sql(before)?,
                                req.preview_sample_limit.min(1000)
                            ),
                            Params::Positional(params),
                        )
                        .await
                        .map_err(error)?,
                        req.preview_sample_limit.min(1000) as usize,
                        "Before edit",
                        true,
                    )
                    .await?
                    .remove(0)
                    .table;
                }
                let mut end = index + 1;
                if matches!(plan.kind, StatementKind::Insert) && !plan.values.is_empty() {
                    while end < plans.len()
                        && end - index < 500
                        && (end - index + 1) * plan.values.len() <= 60000
                        && plans[end].sql == plan.sql
                    {
                        end += 1;
                    }
                }
                let sql = if end - index > 1 {
                    let (prefix, tuple) = sql
                        .rsplit_once(" VALUES ")
                        .ok_or_else(|| DbError::query("Invalid INSERT plan"))?;
                    format!("{prefix} VALUES {}", vec![tuple; end - index].join(","))
                } else {
                    sql
                };
                let params = plans[index..end]
                    .iter()
                    .flat_map(|p| p.values.iter())
                    .map(value::bound)
                    .collect::<Result<Vec<_>, _>>()?;
                conn.exec_drop(&sql, Params::Positional(params))
                    .await
                    .map_err(error)?;
                let affected = conn.affected_rows();
                if !matches!(plan.kind, StatementKind::Insert) && affected != 1 {
                    return Err(DbError::new(ErrorKind::Conflict,"Edit must affect exactly one row; all edits rolled back (unchanged UPDATE is also a conflict)"));
                }
                p.affected = Some(affected);
                samples.push(p);
                changed.statements.push(sql);
                if end - index > 1 {
                    changed.affected.extend(std::iter::repeat_n(1, end - index));
                } else {
                    changed.affected.push(affected);
                }
                index = end;
            }
            Ok((changed,ChangePreview{statements:samples,committed:false,notes:vec!["Edits previewed in a separate connection; all changes rolled back. AUTO_INCREMENT gaps remain.".into()]}))
        };
        let r = self.guarded(id, cancel.clone(), req.timeout_ms, work).await;
        let cancelled = cancel.is_cancelled();
        self.cleanup(
            &mut conn,
            if preview || r.is_err() || cancelled {
                "ROLLBACK"
            } else {
                "COMMIT"
            },
        )
        .await?;
        if cancelled && r.is_ok() {
            return Err(DbError::new(
                ErrorKind::Cancelled,
                "Edits cancelled and rolled back",
            ));
        }
        r
    }
    pub(super) async fn edit_preview(
        &self,
        req: &EditRequest,
        cancel: CancelToken,
    ) -> Result<ChangePreview, DbError> {
        Ok(self.edits(req, true, cancel).await?.1)
    }
    pub(super) async fn edit_apply(
        &self,
        req: &EditRequest,
        cancel: CancelToken,
    ) -> Result<EditResult, DbError> {
        Ok(self.edits(req, false, cancel).await?.0)
    }
    pub(super) async fn plan(
        &self,
        sql: &str,
        analyze: bool,
        cancel: CancelToken,
    ) -> Result<PlanTree, DbError> {
        let ast = parsed(sql)?;
        if ast.len() != 1 {
            return Err(DbError::query("Explain requires exactly one statement"));
        }
        allowed(&ast)?;
        let mut gate = tokio::select! {s=self.conn.lock()=>s,_=cancel.cancelled()=>return Err(DbError::new(ErrorKind::Cancelled,"Explain cancelled"))};
        let mut conn = self.operation_connection(&mut gate).await?;
        if analyze && (self.maria || conn.server_version() < (8, 0, 18)) {
            return Err(DbError::unsupported(
                "EXPLAIN ANALYZE requires MySQL >= 8.0.18; MariaDB ANALYZE is not enabled",
            ));
        }
        self.set_timeout(&mut conn, 30000).await?;
        conn.query_drop("START TRANSACTION").await.map_err(error)?;
        let id = conn.id();
        let work = async {
            transactional_tables(
                &mut conn,
                &ast,
                ast.iter().any(|s| !matches!(s, Statement::Query(_))),
            )
            .await?;
            let raw: String = conn
                .query_first(format!("EXPLAIN FORMAT=JSON {sql}"))
                .await
                .map_err(error)?
                .ok_or_else(|| DbError::query("Missing explain plan"))?;
            let json: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| DbError::query("Invalid explain JSON"))?;
            let mut plan = plan_json("Query", &json);
            if analyze {
                let tree: String = conn
                    .query_first(format!("EXPLAIN ANALYZE FORMAT=TREE {sql}"))
                    .await
                    .map_err(error)?
                    .ok_or_else(|| DbError::query("Missing analyze plan"))?;
                plan = plan_tree(&tree)?;
                plan.warnings.push("EXPLAIN ANALYZE executed inside a rollback transaction; MySQL 8.x returns TREE, not JSON.".into());
            }
            Ok(plan)
        };
        let r = self.guarded(id, cancel, 30000, work).await;
        self.cleanup(&mut conn, "ROLLBACK").await?;
        r
    }
}
fn number(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}
fn plan_json(label: &str, v: &serde_json::Value) -> PlanTree {
    let mut p = PlanTree {
        op: v["access_type"].as_str().unwrap_or(label).into(),
        target: v["table_name"].as_str().map(str::to_owned),
        est_rows: number(&v["rows_examined_per_scan"]).or_else(|| number(&v["rows"])),
        cost: number(&v["cost_info"]["query_cost"])
            .or_else(|| number(&v["cost_info"]["prefix_cost"])),
        ..Default::default()
    };
    if v["access_type"] == "ALL" {
        p.warnings.push("Full table scan".into());
    }
    if let Some(key) = v["key"].as_str() {
        p.warnings.push(format!("Index: {key}"));
    }
    match v {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if (child.is_object() || child.is_array())
                    && !matches!(
                        key.as_str(),
                        "cost_info" | "used_columns" | "possible_keys" | "used_key_parts"
                    )
                {
                    p.children.push(plan_json(key, child));
                }
            }
        }
        serde_json::Value::Array(a) => p.children = a.iter().map(|v| plan_json(label, v)).collect(),
        _ => {}
    }
    p
}
fn plan_tree(raw: &str) -> Result<PlanTree, DbError> {
    fn metric(line: &str, label: &str) -> Option<f64> {
        line.split_once(label)?
            .1
            .split(|c: char| c.is_whitespace() || c == ')')
            .next()?
            .rsplit("..")
            .next()?
            .parse()
            .ok()
    }
    let mut flat: Vec<(usize, PlanTree)> = vec![];
    for line in raw.lines().filter(|l| l.contains("->")) {
        let depth = line.find("->").expect("arrow");
        let text = line[depth + 2..].trim();
        let est = text.split_once(" (actual").map(|p| p.0).unwrap_or(text);
        let actual = text.split_once("(actual").map(|p| p.1).unwrap_or("");
        let mut p = PlanTree {
            op: text
                .split(" (cost=")
                .next()
                .unwrap_or(text)
                .split(" (actual")
                .next()
                .unwrap_or(text)
                .into(),
            target: text
                .split_once(" on ")
                .and_then(|(_, tail)| tail.split_whitespace().next())
                .map(str::to_owned),
            est_rows: metric(est, "rows="),
            actual_rows: metric(actual, "rows="),
            cost: metric(est, "cost="),
            ..Default::default()
        };
        p.warn_estimate();
        flat.push((depth, p));
    }
    fn nest(flat: &[(usize, PlanTree)], i: &mut usize) -> PlanTree {
        let (depth, mut p) = flat[*i].clone();
        *i += 1;
        while *i < flat.len() && flat[*i].0 > depth {
            p.children.push(nest(flat, i));
        }
        p
    }
    if flat.is_empty() {
        return Err(DbError::query("Invalid MySQL analyze tree"));
    }
    Ok(nest(&flat, &mut 0))
}
pub(super) fn ddl(diff: &SnapshotDiff) -> Result<Vec<String>, DbError> {
    if diff.dialect != "mysql" {
        return Err(DbError::unsupported("Snapshot dialect mismatch"));
    }
    let mut sql = vec![];
    for c in &diff.changes {
        match (c.action.as_str(), &c.before, &c.after) {
            ("add", None, Some(o))
                if matches!(o.kind.as_str(), "table" | "view")
                    && !o.definition.trim().is_empty() =>
            {
                sql.push(o.definition.clone())
            }
            ("remove", Some(o), None) if matches!(o.kind.as_str(), "table" | "view") => {
                sql.push(format!(
                    "DROP {} {}",
                    o.kind.to_uppercase(),
                    qualified(&o.namespace, &o.name)
                ))
            }
            ("change", Some(old), Some(new))
                if old.kind == "table"
                    && new.kind == "table"
                    && old.namespace == new.namespace
                    && old.name == new.name =>
            {
                let table = qualified(&old.namespace, &old.name);
                let extract = |definition: &str| -> Result<sqlparser::ast::CreateTable, DbError> {
                    let mut ast = parsed(definition)?;
                    if ast.len() != 1 {
                        return Err(DbError::query("Expected one table definition"));
                    }
                    match ast.remove(0) {
                        Statement::CreateTable(t) => Ok(t),
                        _ => Err(DbError::query("Expected CREATE TABLE definition")),
                    }
                };
                let mut before = extract(&old.definition)?;
                let mut after = extract(&new.definition)?;
                let previous = std::mem::take(&mut before.columns);
                let desired = std::mem::take(&mut after.columns);
                if before != after {
                    return Err(DbError::unsupported(
                        "MySQL indexes/constraints/options changed; write an explicit migration",
                    ));
                }
                for column in &previous {
                    if !desired.iter().any(|c| c.name == column.name) {
                        sql.push(format!(
                            "ALTER TABLE {table} DROP COLUMN {}",
                            quote(&column.name.value)
                        ));
                    }
                }
                for column in &desired {
                    let old = previous.iter().find(|c| c.name == column.name);
                    if old != Some(column) {
                        sql.push(format!(
                            "ALTER TABLE {table} {} COLUMN {column}",
                            if old.is_some() { "MODIFY" } else { "ADD" }
                        ));
                    }
                }
            }
            _ => {
                return Err(DbError::unsupported(
                    "This MySQL schema change needs an explicit migration",
                ))
            }
        }
    }
    Ok(sql)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_sql_rejects_implicit_commit_and_external_effects() {
        for sql in [
            "CREATE TABLE x(id INT)",
            "SET autocommit=1",
            "SELECT * INTO OUTFILE '/tmp/file' FROM x",
            "SELECT dangerous_udf()",
        ] {
            assert!(parsed(sql).and_then(|ast| allowed(&ast)).is_err(), "{sql}");
        }
        assert!(parsed("/*! COMMIT */ SELECT 1").is_err());
        assert!(allowed(&parsed("UPDATE x SET n=n+1 WHERE id=1").unwrap()).is_ok());
    }
    #[test]
    fn normalizes_mysql_and_maria_plans() {
        let p = plan_json(
            "Query",
            &serde_json::json!({"table":{"table_name":"t","access_type":"ALL","rows":100}}),
        );
        assert_eq!(p.children[0].est_rows, Some(100.));
        let p = plan_tree(
            "-> Table scan on t (cost=1.2 rows=100) (actual time=0.1..0.2 rows=2 loops=1)",
        )
        .unwrap();
        assert_eq!(p.actual_rows, Some(2.));
        assert!(!p.warnings.is_empty());
    }
    #[test]
    fn mysql_ddl_keeps_column_attributes_and_never_rebuilds_tables() {
        let object = |definition: &str| SchemaObject {
            namespace: "db".into(),
            name: "items".into(),
            kind: "table".into(),
            definition: definition.into(),
            columns: vec![],
        };
        let diff=SnapshotDiff{dialect:"mysql".into(),changes:vec![SchemaChange{action:"change".into(),before:Some(object("CREATE TABLE `db`.`items` (`id` BIGINT NOT NULL AUTO_INCREMENT, `name` VARCHAR(10) DEFAULT 'x', PRIMARY KEY (`id`)) ENGINE=InnoDB")),after:Some(object("CREATE TABLE `db`.`items` (`id` BIGINT NOT NULL AUTO_INCREMENT, `name` VARCHAR(20) DEFAULT 'x', PRIMARY KEY (`id`)) ENGINE=InnoDB")),risk:SchemaRisk::Destructive}]};
        let rendered = ddl(&diff).unwrap();
        assert_eq!(rendered.len(), 1);
        assert!(rendered[0].starts_with("ALTER TABLE"));
        assert!(rendered[0].contains("VARCHAR(20)"));
        assert!(rendered[0].contains("DEFAULT 'x'"));
    }
    #[test]
    fn edit_operators_do_not_rewrite_quoted_identifiers() {
        let sql=edit_sql("UPDATE `IS NOT DISTINCT FROM` SET `DEFAULT VALUES` = ? WHERE `id` IS NOT DISTINCT FROM ?").unwrap();
        assert!(sql.contains("`IS NOT DISTINCT FROM`"));
        assert!(sql.contains("`DEFAULT VALUES`"));
        assert!(sql.contains("`id` <=> ?"));
    }
}
