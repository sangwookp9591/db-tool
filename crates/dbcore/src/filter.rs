use crate::{DbError, FloatValue, Value};

// Browse uses the drivers' text protocol; accept scalar keys and quote every string.
pub fn sql_literal(value: &Value) -> Result<String, DbError> {
    Ok(match value {
        Value::Null => "NULL".into(),
        Value::Bool(v) => v.to_string(),
        Value::Int(v) => v
            .parse::<i64>()
            .map_err(|_| DbError::query("Invalid integer key"))?
            .to_string(),
        Value::Decimal(v) => {
            if v.is_empty()
                || !v
                    .bytes()
                    .all(|c| c.is_ascii_digit() || b"+-.eE".contains(&c))
                || v.parse::<f64>().is_err()
            {
                return Err(DbError::query("Invalid decimal key"));
            }
            v.clone()
        }
        Value::Float(FloatValue::Number(v)) if v.is_finite() => v.to_string(),
        Value::Text(v)
        | Value::Uuid(v)
        | Value::Date(v)
        | Value::Time(v)
        | Value::DateTime(v)
        | Value::DateTimeTz(v) => {
            if v.contains('\0') {
                return Err(DbError::query("NUL is not a supported key"));
            }
            format!("'{}'", v.replace('\'', "''"))
        }
        _ => {
            return Err(DbError::unsupported(
                "이 값은 데이터 보기 키로 사용할 수 없습니다",
            ))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_are_exact_and_cannot_inject_sql() {
        assert_eq!(
            sql_literal(&Value::Int(i64::MAX.to_string())).unwrap(),
            i64::MAX.to_string()
        );
        assert_eq!(
            sql_literal(&Value::Text("'; DROP TABLE t;--".into())).unwrap(),
            "'''; DROP TABLE t;--'"
        );
        assert!(sql_literal(&Value::Decimal("1; SELECT 1".into())).is_err());
        assert!(sql_literal(&Value::Text("a\0b".into())).is_err());
    }
}
