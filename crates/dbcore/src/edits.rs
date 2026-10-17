//! Parameterized edit planning. Catalog lookup and quoting belong to each driver.
use crate::*;
use std::collections::HashSet;
pub struct Plan {
    pub sql: String,
    pub before: Option<String>,
    pub values: Vec<Value>,
    pub kind: StatementKind,
}
pub fn plan(
    req: &EditRequest,
    table: &str,
    columns: &[String],
    keys: &[String],
    quote: impl Fn(&str) -> String,
    placeholder: impl Fn(usize, &str, &Value) -> String,
) -> Result<Vec<Plan>, DbError> {
    if req.edits.len() > 1000 {
        return Err(DbError::unsupported("At most 1000 edits are supported"));
    }
    if keys.is_empty() {
        return Err(DbError::unsupported("Editing requires a table primary key"));
    }
    req.edits
        .iter()
        .enumerate()
        .map(|(index, edit)| {
            let build = || {
                let (fields, key, kind) = match edit {
                    RowEdit::Update { key, set } => {
                        (set.as_slice(), key.as_slice(), StatementKind::Update)
                    }
                    RowEdit::Insert { values } => {
                        (values.as_slice(), &[][..], StatementKind::Insert)
                    }
                    RowEdit::Delete { key } => (&[][..], key.as_slice(), StatementKind::Delete),
                };
                for pairs in [fields, key] {
                    let mut seen = HashSet::new();
                    for (name, _) in pairs {
                        if !columns.contains(name) || !seen.insert(name) {
                            return Err(DbError::query(format!(
                                "Unknown or duplicate column: {name}"
                            )));
                        }
                    }
                }
                if kind != StatementKind::Insert
                    && (key.len() != keys.len()
                        || keys.iter().any(|k| !key.iter().any(|(n, _)| n == k)))
                {
                    return Err(DbError::query(
                        "Exactly the complete primary key is required",
                    ));
                }
                if kind == StatementKind::Update
                    && (fields.is_empty() || fields.iter().any(|(n, _)| keys.contains(n)))
                {
                    return Err(DbError::query("Update requires non-key columns"));
                }
                let mut values = Vec::new();
                let mut bind = |name: &str, value: &Value| {
                    values.push(value.clone());
                    placeholder(values.len(), name, value)
                };
                let assignments = fields
                    .iter()
                    .map(|(n, v)| format!("{} = {}", quote(n), bind(n, v)))
                    .collect::<Vec<_>>();
                let condition = key
                    .iter()
                    .map(|(n, v)| format!("{} IS NOT DISTINCT FROM {}", quote(n), bind(n, v)))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let sql = match kind {
                    StatementKind::Update => format!(
                        "UPDATE {table} SET {} WHERE {condition}",
                        assignments.join(", ")
                    ),
                    StatementKind::Delete => format!("DELETE FROM {table} WHERE {condition}"),
                    _ if fields.is_empty() => format!("INSERT INTO {table} DEFAULT VALUES"),
                    _ => format!(
                        "INSERT INTO {table} ({}) VALUES ({})",
                        fields
                            .iter()
                            .map(|(n, _)| quote(n))
                            .collect::<Vec<_>>()
                            .join(", "),
                        fields
                            .iter()
                            .enumerate()
                            .map(|(i, (n, v))| placeholder(i + 1, n, v))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                };
                // Before uses the same full parameter list via a SELECT over the DML's key placeholders.
                let before = if key.is_empty() {
                    None
                } else {
                    Some(format!("SELECT * FROM {table} WHERE {condition}"))
                };
                Ok(Plan {
                    sql,
                    before,
                    values,
                    kind,
                })
            };
            build().map_err(|mut e: DbError| {
                e.message = format!("Edit {index}: {}", e.message);
                e
            })
        })
        .collect()
}
pub fn text(value: &Value) -> Result<Option<String>, DbError> {
    Ok(Some(match value {
        Value::Null => return Ok(None),
        Value::Bool(v) => v.to_string(),
        Value::Int(s)
        | Value::Decimal(s)
        | Value::Text(s)
        | Value::Date(s)
        | Value::Time(s)
        | Value::DateTime(s)
        | Value::DateTimeTz(s)
        | Value::Duration(s)
        | Value::Uuid(s)
        | Value::Json(s) => s.clone(),
        Value::Float(FloatValue::Number(v)) => v.to_string(),
        Value::Float(FloatValue::Special(s)) => s.clone(),
        Value::Other { text, .. } => text.clone(),
        Value::Bytes { len, hex }
            if len.checked_mul(2) == Some(hex.len() as u64)
                && hex.bytes().all(|b| b.is_ascii_hexdigit()) =>
        {
            format!("\\x{hex}")
        }
        _ => {
            return Err(DbError::query(
                "Value cannot be converted to a scalar column",
            ))
        }
    }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn additive_contract_defaults_and_roundtrip() {
        let mut result = serde_json::to_value(ResultSet::default()).unwrap();
        result.as_object_mut().unwrap().remove("edit_target");
        assert!(serde_json::from_value::<ResultSet>(result)
            .unwrap()
            .edit_target
            .is_none());
        let req:EditRequest=serde_json::from_value(serde_json::json!({"node_id":"table:t","edits":[{"op":"Update","key":[["id",{"t":"Int","v":"1"}]],"set":[["t",{"t":"Null"}]]}]})).unwrap();
        assert_eq!(req.timeout_ms, 30000);
        assert_eq!(req.preview_sample_limit, 50);
        assert_eq!(
            serde_json::from_value::<EditRequest>(serde_json::to_value(&req).unwrap()).unwrap(),
            req
        );
        assert_eq!(
            serde_json::to_value(ErrorKind::Conflict).unwrap(),
            "Conflict"
        );
    }
}
