use super::*;
type ColumnRow = (
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
);
#[derive(Clone)]
pub(super) struct ColumnInfo {
    pub name: String,
    pub ty: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub pk: bool,
    pub generated: bool,
    pub json: bool,
}
pub(super) async fn database(conn: &mut Conn, scope: Option<&str>) -> Result<String, DbError> {
    if let Some(db) = scope {
        return Ok(db.into());
    }
    conn.query_first::<Option<String>, _>("SELECT DATABASE()")
        .await
        .map_err(error)?
        .flatten()
        .ok_or_else(|| DbError::query("Select a database in the connection form or explorer"))
}
pub(super) async fn columns(
    conn: &mut Conn,
    db: &str,
    table: &str,
    maria: bool,
) -> Result<Vec<ColumnInfo>, DbError> {
    let rows:Vec<ColumnRow>=conn.exec("SELECT COLUMN_NAME,COLUMN_TYPE,IS_NULLABLE,COLUMN_DEFAULT,COLUMN_KEY,EXTRA,DATA_TYPE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA=? AND TABLE_NAME=? ORDER BY ORDINAL_POSITION",(db,table)).await.map_err(error)?;
    if rows.is_empty() {
        return Err(DbError::query("Table does not exist or is not visible"));
    }
    let definition = if maria {
        show_create(conn, db, table).await?.to_ascii_lowercase()
    } else {
        String::new()
    };
    Ok(rows
        .into_iter()
        .map(|(name, ty, null, default, key, extra, data)| {
            let json = data == "json"
                || (maria
                    && definition.contains(&format!(
                        "json_valid({})",
                        quote(&name).to_ascii_lowercase()
                    )));
            ColumnInfo {
                name,
                ty: if json { "JSON".into() } else { ty },
                nullable: null == "YES",
                default,
                pk: key == "PRI",
                generated: extra.contains("GENERATED"),
                json,
            }
        })
        .collect())
}
pub(super) async fn show_create(conn: &mut Conn, db: &str, table: &str) -> Result<String, DbError> {
    let row = conn
        .query_first::<mysql_async::Row, _>(format!("SHOW CREATE TABLE {}", qualified(db, table)))
        .await
        .map_err(error)?
        .ok_or_else(|| DbError::query("Missing table definition"))?;
    row.get_opt::<String, _>(1)
        .ok_or_else(|| DbError::query("Missing table definition"))?
        .map_err(|_| DbError::query("Invalid table definition encoding"))
}
fn schema_definition(sql: &str) -> Result<String, DbError> {
    use sqlparser::tokenizer::{Token, Tokenizer};
    let tokens = Tokenizer::new(&MySqlDialect {}, sql)
        .tokenize_with_location()
        .map_err(|_| DbError::query("Invalid schema definition tokens"))?;
    let tokens = tokens
        .iter()
        .filter(|t| !matches!(t.token, Token::Whitespace(_)))
        .collect::<Vec<_>>();
    let offset = |location: sqlparser::tokenizer::Location| {
        let start = sql
            .split_inclusive('\n')
            .take(location.line.saturating_sub(1) as usize)
            .map(str::len)
            .sum::<usize>();
        start
            + sql[start..]
                .char_indices()
                .nth(location.column.saturating_sub(1) as usize)
                .map(|(i, _)| i)
                .unwrap_or(sql.len() - start)
    };
    let mut depth = 0usize;
    for (i, token) in tokens.iter().enumerate() {
        match token.token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0
            && matches!(&token.token,Token::Word(w) if w.quote_style.is_none()&&w.value.eq_ignore_ascii_case("AUTO_INCREMENT"))
            && tokens.get(i + 1).is_some_and(|t| t.token == Token::Eq)
            && let Some(value) = tokens.get(i + 2).filter(
                |t| matches!(&t.token,Token::Number(n,_) if n.bytes().all(|b|b.is_ascii_digit())),
            ) {
            let mut start = offset(token.span.start);
            let end = offset(value.span.end);
            if start > 0 && sql.as_bytes()[start - 1] == b' ' {
                start -= 1;
            }
            return Ok(format!("{}{}", &sql[..start], &sql[end..]));
        }
    }
    Ok(sql.into())
}
fn node(
    parts: &[&str],
    label: &str,
    kind: &str,
    icon: Icon,
    children: bool,
    detail: Option<String>,
) -> ExplorerNode {
    ExplorerNode {
        id: node_id(parts),
        label: label.into(),
        kind: kind.into(),
        icon,
        has_children: children,
        detail,
        actions: vec![],
    }
}
impl MysqlSession {
    pub(super) async fn children(
        &self,
        parent: Option<&str>,
    ) -> Result<Vec<ExplorerNode>, DbError> {
        let mut conn = self.fresh().await?;
        let mut out = vec![];
        let Some(parent) = parent else {
            let dbs:Vec<String>=conn.query("SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME NOT IN ('information_schema','mysql','performance_schema','sys') ORDER BY SCHEMA_NAME").await.map_err(error)?;
            for db in dbs {
                let mut n = node(
                    &["database", &db],
                    &db,
                    "database",
                    Icon::Database,
                    true,
                    None,
                );
                n.actions.push(NodeAction {
                    id: "diagram".into(),
                    label: "Schema diagram".into(),
                    kind: NodeActionKind::OpenSchemaGraph { scope: Some(db) },
                });
                out.push(n);
            }
            return Ok(out);
        };
        let p: Vec<String> =
            serde_json::from_str(parent).map_err(|_| DbError::query("Invalid MySQL node"))?;
        match p.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
            ["database", db] => {
                for (folder, label) in [("tables", "Tables"), ("views", "Views")] {
                    out.push(node(
                        &["folder", db, folder],
                        label,
                        "folder",
                        Icon::Folder,
                        true,
                        None,
                    ));
                }
            }
            ["folder", db, kind] if matches!(*kind, "tables" | "views") => {
                let names:Vec<String>=conn.exec("SELECT TABLE_NAME FROM information_schema.TABLES WHERE TABLE_SCHEMA=? AND TABLE_TYPE=? ORDER BY TABLE_NAME",(db,if *kind=="tables"{"BASE TABLE"}else{"VIEW"})).await.map_err(error)?;
                for name in names {
                    let mut n = node(
                        &["table", db, &name],
                        &name,
                        if *kind == "views" { "view" } else { "table" },
                        if *kind == "views" {
                            Icon::View
                        } else {
                            Icon::Table
                        },
                        true,
                        None,
                    );
                    n.actions = vec![
                        NodeAction {
                            id: "browse".into(),
                            label: "Browse".into(),
                            kind: NodeActionKind::Browse,
                        },
                        NodeAction {
                            id: "query".into(),
                            label: "Open query".into(),
                            kind: NodeActionKind::OpenQuery {
                                text: format!("SELECT * FROM {} LIMIT 100", qualified(db, &name)),
                            },
                        },
                    ];
                    out.push(n);
                }
            }
            ["table", db, table] => {
                for (kind, label, icon) in [
                    ("columns", "Columns", Icon::Column),
                    ("indexes", "Indexes", Icon::Index),
                    ("foreign_keys", "Foreign keys", Icon::Relationship),
                ] {
                    out.push(node(&[kind, db, table], label, "folder", icon, true, None));
                }
            }
            ["columns", db, table] => {
                for c in columns(&mut conn, db, table, self.maria).await? {
                    out.push(node(
                        &["column", db, table, &c.name],
                        &c.name,
                        "column",
                        if c.pk { Icon::Key } else { Icon::Column },
                        false,
                        Some(format!(
                            "{} · {}{}",
                            c.ty,
                            if c.nullable { "NULL" } else { "NOT NULL" },
                            if c.pk { " · PK" } else { "" }
                        )),
                    ));
                }
            }
            ["indexes", db, table] => {
                let rows:Vec<(String,u64,String)>=conn.exec("SELECT INDEX_NAME,NON_UNIQUE,GROUP_CONCAT(COALESCE(COLUMN_NAME,'<expression>') ORDER BY SEQ_IN_INDEX SEPARATOR ', ') FROM information_schema.STATISTICS WHERE TABLE_SCHEMA=? AND TABLE_NAME=? GROUP BY INDEX_NAME,NON_UNIQUE ORDER BY INDEX_NAME",(db,table)).await.map_err(error)?;
                for (name, nonunique, fields) in rows {
                    out.push(node(
                        &["index", db, table, &name],
                        &name,
                        "index",
                        Icon::Index,
                        false,
                        Some(format!(
                            "{}({fields})",
                            if nonunique == 0 { "UNIQUE " } else { "" }
                        )),
                    ));
                }
            }
            ["foreign_keys", db, table] => {
                for r in foreign_keys(&mut conn, db, Some(table)).await?.relations {
                    out.push(node(
                        &["fk", db, table, &r.name],
                        &r.name,
                        "foreign_key",
                        Icon::Relationship,
                        false,
                        Some(format!(
                            "{} → {} ({})",
                            r.from_fields.join(","),
                            r.to,
                            r.to_fields.join(",")
                        )),
                    ));
                }
            }
            _ => return Err(DbError::query("Invalid MySQL explorer node")),
        }
        Ok(out)
    }
    pub(super) async fn browse_table(
        &self,
        node: &str,
        page: PageRequest,
    ) -> Result<ResultSet, DbError> {
        let (db, table) = table_path(node)?;
        let mut conn = self.fresh().await?;
        let cols = columns(&mut conn, &db, &table, self.maria).await?;
        let keys = cols
            .iter()
            .filter(|c| c.pk)
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        let mut params = vec![];
        let mut predicates = vec![];
        for (field, value) in &page.filter {
            let column = cols
                .iter()
                .find(|c| c.name == *field)
                .ok_or_else(|| DbError::query("Unknown filter column"))?;
            let predicate = match value {
                Value::Json(_) if column.json && self.maria => {
                    format!("JSON_EQUALS({},?)", quote(field))
                }
                Value::Json(_) if column.json => format!("{} <=> CAST(? AS JSON)", quote(field)),
                Value::Json(_) => return Err(DbError::query("JSON filters require a JSON column")),
                Value::Decimal(text) => format!(
                    "{} <=> CAST(? AS {})",
                    quote(field),
                    value::decimal_type(text)?
                ),
                _ => format!("{} <=> ?", quote(field)),
            };
            predicates.push(predicate);
            params.push(value::bound(value)?);
        }
        let limit = clamp_row_limit(page.limit);
        let condition = if predicates.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", predicates.join(" AND "))
        };
        let order = if keys.is_empty() {
            quote(&cols[0].name)
        } else {
            keys.iter().map(|k| quote(k)).collect::<Vec<_>>().join(",")
        };
        let sql = format!(
            "SELECT * FROM {}{condition} ORDER BY {order} LIMIT {} OFFSET {}",
            qualified(&db, &table),
            limit + 1,
            page.offset
        );
        self.set_timeout(&mut conn, 30000).await?;
        let id = conn.id();
        let work = async {
            collect(
                conn.exec_iter(&sql, Params::Positional(params))
                    .await
                    .map_err(error)?,
                limit as usize,
                &sql,
                true,
            )
            .await
        };
        let mut r = self
            .guarded(id, CancelToken::new(), 30000, work)
            .await?
            .remove(0);
        if let Some(t) = &mut r.table {
            for (i, c) in cols.iter().enumerate() {
                t.columns[i].type_name = c.ty.clone();
                if c.json {
                    for row in &mut t.rows {
                        if let Value::Text(s) = &row[i] {
                            row[i] = Value::Json(s.clone());
                        }
                    }
                }
            }
        }
        r.page = Some(PageInfo {
            offset: page.offset,
            limit,
            has_more: r.truncated,
        });
        if !keys.is_empty() {
            r.edit_target = Some(EditTarget {
                node_id: node.into(),
                key_columns: keys,
            });
        }
        Ok(r)
    }
    pub(super) async fn graph(&self, scope: Option<&str>) -> Result<SchemaGraph, DbError> {
        let mut conn = self.fresh().await?;
        let db = database(&mut conn, scope).await?;
        let tables:Vec<(String,String)>=conn.exec("SELECT TABLE_NAME,TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA=? ORDER BY TABLE_NAME",(&db,)).await.map_err(error)?;
        let mut g = foreign_keys(&mut conn, &db, None).await?;
        for (table, ty) in tables {
            let cols = columns(&mut conn, &db, &table, self.maria).await?;
            g.entities.push(SchemaEntity {
                id: node_id(&["table", &db, &table]),
                name: table,
                kind: if ty == "VIEW" { "view" } else { "table" }.into(),
                fields: cols
                    .into_iter()
                    .map(|c| SchemaField {
                        name: c.name,
                        type_name: c.ty,
                        pk: c.pk,
                        nullable: c.nullable,
                    })
                    .collect(),
            });
        }
        let external = g
            .relations
            .iter()
            .map(|r| r.to.clone())
            .filter(|id| !g.entities.iter().any(|e| e.id == *id))
            .collect::<std::collections::BTreeSet<_>>();
        for id in external {
            let (db, table) = table_path(&id)?;
            let cols = columns(&mut conn, &db, &table, self.maria).await?;
            g.entities.push(SchemaEntity {
                id,
                name: format!("{db}.{table}"),
                kind: "table".into(),
                fields: cols
                    .into_iter()
                    .map(|c| SchemaField {
                        name: c.name,
                        type_name: c.ty,
                        pk: c.pk,
                        nullable: c.nullable,
                    })
                    .collect(),
            });
        }
        if g.entities.len() > 50000 {
            return Err(DbError::query("Schema graph exceeds 50000 entities"));
        }
        Ok(g)
    }
    pub(super) async fn snapshot(&self, scope: Option<&str>) -> Result<SchemaSnapshot, DbError> {
        let mut conn = self.fresh().await?;
        let db = database(&mut conn, scope).await?;
        let rows:Vec<(String,String)>=conn.exec("SELECT TABLE_NAME,TABLE_TYPE FROM information_schema.TABLES WHERE TABLE_SCHEMA=? ORDER BY TABLE_NAME LIMIT 50001",(&db,)).await.map_err(error)?;
        let mut s = SchemaSnapshot {
            version: 1,
            dialect: "mysql".into(),
            objects: vec![],
        };
        for (name, ty) in rows {
            let cols = columns(&mut conn, &db, &name, self.maria)
                .await?
                .into_iter()
                .map(|c| SnapshotColumn {
                    name: c.name,
                    type_name: c.ty,
                    nullable: c.nullable,
                    default: c.default,
                })
                .collect();
            let mut def = show_create(&mut conn, &db, &name).await?;
            // SHOW CREATE uses an unqualified table name; snapshots must remain portable across sessions.
            if ty != "VIEW" {
                def = schema_definition(&def)?.replacen(
                    &format!("CREATE TABLE {}", quote(&name)),
                    &format!("CREATE TABLE {}", qualified(&db, &name)),
                    1,
                );
            }
            s.objects.push(SchemaObject {
                namespace: db.clone(),
                name,
                kind: if ty == "VIEW" { "view" } else { "table" }.into(),
                definition: def,
                columns: cols,
            });
        }
        s.normalize()?;
        Ok(s)
    }
}
async fn foreign_keys(
    conn: &mut Conn,
    db: &str,
    table: Option<&str>,
) -> Result<SchemaGraph, DbError> {
    let rows:Vec<(String,String,String,String,String,String)>=conn.exec("SELECT CONSTRAINT_NAME,TABLE_NAME,COLUMN_NAME,REFERENCED_TABLE_SCHEMA,REFERENCED_TABLE_NAME,REFERENCED_COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA=? AND REFERENCED_TABLE_NAME IS NOT NULL AND (? IS NULL OR TABLE_NAME=?) ORDER BY TABLE_NAME,CONSTRAINT_NAME,ORDINAL_POSITION",(db,table,table)).await.map_err(error)?;
    let mut relations: Vec<SchemaRelation> = vec![];
    for (name, from, field, to_db, to, to_field) in rows {
        let id = node_id(&["fk", db, &from, &name]);
        if let Some(r) = relations.iter_mut().find(|r| r.id == id) {
            r.from_fields.push(field);
            r.to_fields.push(to_field);
        } else {
            relations.push(SchemaRelation {
                id,
                name,
                from: node_id(&["table", db, &from]),
                from_fields: vec![field],
                to: node_id(&["table", &to_db, &to]),
                to_fields: vec![to_field],
                kind: "foreign_key".into(),
            });
        }
    }
    Ok(SchemaGraph {
        entities: vec![],
        relations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_exclude_only_the_volatile_auto_increment_counter() {
        let sql="CREATE TABLE `한글` (`id` BIGINT AUTO_INCREMENT, `n` TEXT COMMENT ' AUTO_INCREMENT=99') ENGINE=InnoDB AUTO_INCREMENT=42 DEFAULT CHARSET=utf8mb4";
        assert_eq!(schema_definition(sql).unwrap(),"CREATE TABLE `한글` (`id` BIGINT AUTO_INCREMENT, `n` TEXT COMMENT ' AUTO_INCREMENT=99') ENGINE=InnoDB DEFAULT CHARSET=utf8mb4");
    }
}
