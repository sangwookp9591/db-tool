//! Portable management contracts. Plans never execute database operations.
use crate::*;
use std::collections::BTreeMap;

pub const IMPORT_MAX_ROWS: usize = 10_000;
pub const IMPORT_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// New import tables accept only portable mapped types, never caller-provided SQL.
pub fn import_create_sql(
    req: &EditRequest,
    target: &str,
    dialect: &str,
) -> Result<Option<String>, DbError> {
    let Some(columns) = &req.create_table else {
        return Ok(None);
    };
    if columns.is_empty()
        || columns.len() > 512
        || req.edits.is_empty()
        || req
            .edits
            .iter()
            .any(|e| !matches!(e, RowEdit::Insert { .. }))
    {
        return Err(DbError::query(
            "New tables require 1..512 columns and inserts only",
        ));
    }
    let mut names = std::collections::HashSet::new();
    let definitions = columns
        .iter()
        .map(|column| {
            if column.name.is_empty() || column.name.contains('\0') || !names.insert(&column.name) {
                return Err(DbError::query("Invalid or duplicate new table column"));
            }
            let ty = match column.type_name.as_str() {
                "text" => "TEXT",
                "integer" => "BIGINT",
                "float" => "DOUBLE PRECISION",
                "boolean" => "BOOLEAN",
                "json" if dialect == "postgres" => "JSONB",
                "json" if dialect == "sqlite" => "TEXT",
                _ => return Err(DbError::query("Unsupported import column type")),
            };
            Ok(format!("{} {ty}", quote_identifier(&column.name)))
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    Ok(Some(format!(
        "CREATE TABLE {target} ({})",
        definitions.join(", ")
    )))
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SchemaSnapshot {
    pub version: u32,
    pub dialect: String,
    pub objects: Vec<SchemaObject>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaObject {
    pub namespace: String,
    pub name: String,
    pub kind: String,
    pub definition: String,
    pub columns: Vec<SnapshotColumn>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotColumn {
    pub name: String,
    pub type_name: String,
    pub nullable: bool,
    pub default: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaRisk {
    Read,
    Write,
    Destructive,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaChange {
    pub action: String,
    pub before: Option<SchemaObject>,
    pub after: Option<SchemaObject>,
    pub risk: SchemaRisk,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotDiff {
    pub dialect: String,
    pub changes: Vec<SchemaChange>,
}
impl SchemaSnapshot {
    pub fn normalize(&mut self) -> Result<(), DbError> {
        if self.version != 1 || self.objects.len() > 50_000 {
            return Err(DbError::query("Unsupported or oversized schema snapshot"));
        }
        self.objects.sort_by(|a, b| {
            (&a.namespace, &a.kind, &a.name).cmp(&(&b.namespace, &b.kind, &b.name))
        });
        if self.objects.windows(2).any(|p| {
            (&p[0].namespace, &p[0].kind, &p[0].name) == (&p[1].namespace, &p[1].kind, &p[1].name)
        }) {
            return Err(DbError::query("Duplicate schema object"));
        }
        Ok(())
    }
}
pub fn snapshot_diff(
    before: &SchemaSnapshot,
    after: &SchemaSnapshot,
) -> Result<SnapshotDiff, DbError> {
    if before.dialect != after.dialect {
        return Err(DbError::unsupported("Cross-dialect DDL is not supported"));
    }
    let (mut before, mut after) = (before.clone(), after.clone());
    before.normalize()?;
    after.normalize()?;
    let key = |o: &SchemaObject| (o.namespace.clone(), o.kind.clone(), o.name.clone());
    let old = before
        .objects
        .iter()
        .map(|o| (key(o), o))
        .collect::<BTreeMap<_, _>>();
    let new = after
        .objects
        .iter()
        .map(|o| (key(o), o))
        .collect::<BTreeMap<_, _>>();
    let mut changes = vec![];
    for (k, o) in &old {
        match new.get(k) {
            None => changes.push(SchemaChange {
                action: "remove".into(),
                before: Some((*o).clone()),
                after: None,
                risk: SchemaRisk::Destructive,
            }),
            Some(n) if *o != *n => changes.push(SchemaChange {
                action: "change".into(),
                before: Some((*o).clone()),
                after: Some((*n).clone()),
                risk: SchemaRisk::Destructive,
            }),
            _ => {}
        }
    }
    for (k, n) in new {
        if !old.contains_key(&k) {
            changes.push(SchemaChange {
                action: "add".into(),
                before: None,
                after: Some(n.clone()),
                risk: SchemaRisk::Write,
            });
        }
    }
    Ok(SnapshotDiff {
        dialect: before.dialect,
        changes,
    })
}
pub fn quote_identifier(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn drop_sql(object: &SchemaObject) -> Result<String, DbError> {
    if object.kind == "constraint" {
        let tokens = crate::preview::tokens(&object.definition)?;
        if tokens.len() > 5
            && tokens[0].is("ALTER")
            && tokens[1].is("TABLE")
            && let Some(i) = tokens
                .windows(2)
                .position(|t| t[0].is("ADD") && t[1].is("CONSTRAINT"))
            && let Some(name) = tokens.get(i + 2)
        {
            return Ok(format!(
                "{}DROP CONSTRAINT {}",
                &object.definition[..tokens[i].start],
                name.text
            ));
        }
        return Err(DbError::query("Invalid constraint definition"));
    }
    if !["table", "view", "index", "materialized view", "trigger"].contains(&object.kind.as_str()) {
        return Err(DbError::unsupported("Unsupported schema object removal"));
    }
    Ok(format!(
        "DROP {} {}{}",
        object.kind.to_uppercase(),
        if object.namespace.is_empty() {
            String::new()
        } else {
            format!("{}.", quote_identifier(&object.namespace))
        },
        quote_identifier(&object.name)
    ))
}
/// Snapshot SQL is untrusted; generated DDL still requires preview and approval.
pub fn render_sql(diff: &SnapshotDiff, dialect: &str) -> Result<Vec<String>, DbError> {
    if diff.dialect != dialect {
        return Err(DbError::unsupported("Snapshot dialect mismatch"));
    }
    let mut removals = vec![];
    let mut additions = vec![];
    let mut alterations = vec![];
    for change in &diff.changes {
        match (change.action.as_str(), &change.before, &change.after) {
            ("add", None, Some(object)) if !object.definition.trim().is_empty() => {
                additions.push(object)
            }
            ("remove", Some(object), None) => removals.push(object),
            ("change", Some(old), Some(new)) if old.kind == new.kind && old.kind == "table" => {
                let target = format!(
                    "{}{}",
                    if old.namespace.is_empty() {
                        String::new()
                    } else {
                        format!("{}.", quote_identifier(&old.namespace))
                    },
                    quote_identifier(&old.name)
                );
                if old.columns == new.columns && old.definition != new.definition {
                    return Err(DbError::unsupported(
                        "Table options changed; write an explicit migration",
                    ));
                }
                for column in &old.columns {
                    if !new.columns.iter().any(|c| c.name == column.name) {
                        alterations.push(format!(
                            "ALTER TABLE {target} DROP COLUMN {}",
                            quote_identifier(&column.name)
                        ));
                    }
                }
                for column in &new.columns {
                    let name = quote_identifier(&column.name);
                    match old.columns.iter().find(|c| c.name == column.name) {
                        None => alterations.push(format!("ALTER TABLE {target} ADD COLUMN {name} {}{}{}", column.type_name,
                            if column.nullable { "" } else { " NOT NULL" },
                            column.default.as_ref().map(|d| format!(" DEFAULT {d}")).unwrap_or_default())),
                        Some(previous) if previous != column && dialect == "sqlite" => return Err(DbError::unsupported("SQLite type/default/nullability changes require an explicit table rebuild migration")),
                        Some(previous) => {
                            if previous.type_name != column.type_name { alterations.push(format!("ALTER TABLE {target} ALTER COLUMN {name} TYPE {}", column.type_name)); }
                            if previous.nullable != column.nullable { alterations.push(format!("ALTER TABLE {target} ALTER COLUMN {name} {} NOT NULL", if column.nullable { "DROP" } else { "SET" })); }
                            if previous.default != column.default { alterations.push(format!("ALTER TABLE {target} ALTER COLUMN {name} {}", column.default.as_ref().map(|d| format!("SET DEFAULT {d}")).unwrap_or_else(|| "DROP DEFAULT".into()))); }
                        }
                    }
                }
            }
            ("change", Some(old), Some(new))
                if old.kind == new.kind && !new.definition.trim().is_empty() =>
            {
                removals.push(old);
                additions.push(new);
            }
            _ => {
                return Err(DbError::unsupported(
                    "This schema change requires an explicit migration",
                ))
            }
        }
    }
    let rank = |o: &&SchemaObject| match o.kind.as_str() {
        "table" => 0,
        "index" => 1,
        "constraint" => 2,
        _ => 3,
    };
    removals.sort_by_key(|o| std::cmp::Reverse(rank(o)));
    additions.sort_by_key(rank);
    let mut sql = removals
        .into_iter()
        .map(drop_sql)
        .collect::<Result<Vec<_>, _>>()?;
    sql.extend(alterations);
    sql.extend(additions.into_iter().map(|o| o.definition.clone()));
    Ok(sql)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportMapping {
    pub source: String,
    pub target: String,
    pub type_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportError {
    pub row: u64,
    pub column: Option<String>,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportPlan {
    pub format: String,
    pub columns: Vec<ImportMapping>,
    pub sample: Vec<Vec<Value>>,
    pub row_count: u64,
    pub errors: Vec<ImportError>,
    pub warnings: Vec<String>,
    pub request: EditRequest,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(ty: &str) -> SchemaSnapshot {
        SchemaSnapshot {
            version: 1,
            dialect: "postgres".into(),
            objects: vec![SchemaObject {
                namespace: "public".into(),
                name: "a\"b".into(),
                kind: "table".into(),
                definition: ty.into(),
                columns: vec![SnapshotColumn {
                    name: "x".into(),
                    type_name: ty.into(),
                    nullable: true,
                    default: None,
                }],
            }],
        }
    }
    #[test]
    fn changes_are_destructive_and_identifiers_quoted() {
        let d = snapshot_diff(&snapshot("text"), &snapshot("integer")).unwrap();
        assert_eq!(d.changes[0].risk, SchemaRisk::Destructive);
        assert_eq!(
            render_sql(&d, "postgres").unwrap(),
            vec!["ALTER TABLE \"public\".\"a\"\"b\" ALTER COLUMN \"x\" TYPE integer"]
        );
        assert!(render_sql(&d, "sqlite").is_err());
    }
    #[test]
    fn sqlite_columns_and_postgres_constraints_render_without_table_rebuilds() {
        let mut before = snapshot("TEXT");
        before.dialect = "sqlite".into();
        let mut after = before.clone();
        after.objects[0].columns.push(SnapshotColumn {
            name: "new".into(),
            type_name: "TEXT".into(),
            nullable: true,
            default: None,
        });
        assert!(
            render_sql(&snapshot_diff(&before, &after).unwrap(), "sqlite").unwrap()[0]
                .contains("ADD COLUMN \"new\" TEXT")
        );
        let object = SchemaObject {
            namespace: "public".into(),
            name: "a.b.constraint.with.dot".into(),
            kind: "constraint".into(),
            definition:
                "ALTER TABLE \"public\".\"a.b\" ADD CONSTRAINT \"constraint.with.dot\" UNIQUE (x)"
                    .into(),
            columns: vec![],
        };
        assert_eq!(
            drop_sql(&object).unwrap(),
            "ALTER TABLE \"public\".\"a.b\" DROP CONSTRAINT \"constraint.with.dot\""
        );
    }
    #[test]
    fn new_import_table_types_are_allowlisted() {
        let mut req = EditRequest {originals: vec![],
            node_id: "table:new".into(),
            create_table: None,
            edits: vec![RowEdit::Insert { values: vec![] }],
            timeout_ms: 1000,
            preview_sample_limit: 100,
        };
        req.create_table = Some(vec![Column {
            name: "safe".into(),
            type_name: "TEXT); DROP TABLE users;--".into(),
        }]);
        assert!(import_create_sql(&req, "new", "sqlite").is_err());
        req.create_table.as_mut().unwrap()[0].type_name = "integer".into();
        assert_eq!(
            import_create_sql(&req, "new", "sqlite").unwrap().unwrap(),
            "CREATE TABLE new (\"safe\" BIGINT)"
        );
    }
    #[test]
    fn duplicate_objects_and_cross_dialect_rejected() {
        let mut s = snapshot("text");
        s.objects.push(s.objects[0].clone());
        assert!(s.normalize().is_err());
        let mut t = snapshot("text");
        t.dialect = "sqlite".into();
        assert!(snapshot_diff(&snapshot("text"), &t).is_err());
    }
}
