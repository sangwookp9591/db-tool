use super::*;
use mysql_async::{
    consts::{ColumnFlags, ColumnType},
    Column as MyColumn, Value as MyValue,
};
fn utf8(bytes: &[u8]) -> Result<String, DbError> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| DbError::query("Invalid UTF-8 in MySQL text column"))
}
pub(super) fn type_name(c: &MyColumn) -> String {
    let name = match c.column_type() {
        ColumnType::MYSQL_TYPE_NULL => "NULL",
        ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => "DECIMAL",
        ColumnType::MYSQL_TYPE_TINY => "TINYINT",
        ColumnType::MYSQL_TYPE_SHORT => "SMALLINT",
        ColumnType::MYSQL_TYPE_LONG => "INT",
        ColumnType::MYSQL_TYPE_LONGLONG => "BIGINT",
        ColumnType::MYSQL_TYPE_INT24 => "MEDIUMINT",
        ColumnType::MYSQL_TYPE_FLOAT => "FLOAT",
        ColumnType::MYSQL_TYPE_DOUBLE => "DOUBLE",
        ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE => "DATE",
        ColumnType::MYSQL_TYPE_TIME | ColumnType::MYSQL_TYPE_TIME2 => "TIME",
        ColumnType::MYSQL_TYPE_TIMESTAMP | ColumnType::MYSQL_TYPE_TIMESTAMP2 => "TIMESTAMP (UTC)",
        ColumnType::MYSQL_TYPE_DATETIME | ColumnType::MYSQL_TYPE_DATETIME2 => {
            "DATETIME (no time zone)"
        }
        ColumnType::MYSQL_TYPE_JSON => "JSON",
        ColumnType::MYSQL_TYPE_BIT => "BIT",
        ColumnType::MYSQL_TYPE_YEAR => "YEAR",
        _ if c.flags().contains(ColumnFlags::ENUM_FLAG) => "ENUM",
        _ if c.flags().contains(ColumnFlags::SET_FLAG) => "SET",
        _ if c.character_set() == 63 => "BINARY/BLOB",
        _ => "TEXT",
    };
    format!(
        "{name}{}",
        if c.flags().contains(ColumnFlags::UNSIGNED_FLAG)
            && matches!(
                name,
                "TINYINT" | "SMALLINT" | "INT" | "BIGINT" | "MEDIUMINT" | "DECIMAL"
            )
        {
            " UNSIGNED"
        } else {
            ""
        }
    )
}
pub(super) fn cell(c: &MyColumn, v: &MyValue) -> Result<Value, DbError> {
    let bytes = match v {
        MyValue::NULL => return Ok(Value::Null),
        MyValue::Int(n) => return Ok(Value::Int(n.to_string())),
        MyValue::UInt(n) => return Ok(Value::Int(n.to_string())),
        MyValue::Float(n) => return float(*n as f64),
        MyValue::Double(n) => return float(*n),
        MyValue::Date(y, m, d, h, min, s, us) => {
            if matches!(
                c.column_type(),
                ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE
            ) {
                return Ok(Value::Date(format!("{y:04}-{m:02}-{d:02}")));
            }
            let text = format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}:{s:02}.{us:06}");
            return Ok(
                if matches!(
                    c.column_type(),
                    ColumnType::MYSQL_TYPE_TIMESTAMP | ColumnType::MYSQL_TYPE_TIMESTAMP2
                ) {
                    Value::DateTimeTz(format!("{text}+00:00"))
                } else {
                    Value::DateTime(text)
                },
            );
        }
        MyValue::Time(neg, days, h, m, s, us) => {
            return Ok(Value::Time(format!(
                "{}{:02}:{m:02}:{s:02}.{us:06}",
                if *neg { "-" } else { "" },
                *days as u64 * 24 + *h as u64
            )))
        }
        MyValue::Bytes(bytes) => bytes,
    };
    use ColumnType::*;
    Ok(match c.column_type() {
        MYSQL_TYPE_BIT => {
            if bytes.len() > 8 {
                return Err(DbError::query("MySQL BIT exceeds 64 bits"));
            }
            Value::Int(
                bytes
                    .iter()
                    .fold(0u64, |n, b| (n << 8) | *b as u64)
                    .to_string(),
            )
        }
        MYSQL_TYPE_DECIMAL | MYSQL_TYPE_NEWDECIMAL => Value::Decimal(utf8(bytes)?),
        MYSQL_TYPE_TINY | MYSQL_TYPE_SHORT | MYSQL_TYPE_LONG | MYSQL_TYPE_LONGLONG
        | MYSQL_TYPE_INT24 | MYSQL_TYPE_YEAR => Value::Int(utf8(bytes)?),
        MYSQL_TYPE_FLOAT | MYSQL_TYPE_DOUBLE => {
            return float(
                utf8(bytes)?
                    .parse()
                    .map_err(|_| DbError::query("Invalid MySQL float"))?,
            )
        }
        MYSQL_TYPE_DATE | MYSQL_TYPE_NEWDATE => Value::Date(utf8(bytes)?),
        MYSQL_TYPE_TIME | MYSQL_TYPE_TIME2 => Value::Time(utf8(bytes)?),
        MYSQL_TYPE_DATETIME | MYSQL_TYPE_DATETIME2 => Value::DateTime(utf8(bytes)?),
        MYSQL_TYPE_TIMESTAMP | MYSQL_TYPE_TIMESTAMP2 => {
            Value::DateTimeTz(format!("{}+00:00", utf8(bytes)?))
        }
        MYSQL_TYPE_JSON => Value::Json(utf8(bytes)?),
        _ if c.character_set() == 63 => Value::Bytes {
            len: bytes.len() as u64,
            hex: hex(&bytes[..bytes.len().min(64)]),
        },
        _ => Value::Text(utf8(bytes)?),
    })
}
fn float(n: f64) -> Result<Value, DbError> {
    Ok(Value::Float(if n.is_finite() {
        FloatValue::Number(n)
    } else {
        FloatValue::Special(
            if n.is_nan() {
                "NaN"
            } else if n.is_sign_negative() {
                "-Infinity"
            } else {
                "Infinity"
            }
            .into(),
        )
    }))
}
pub(super) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // INVARIANT: formatting into a String cannot fail.
        #[allow(clippy::expect_used)]
        write!(s, "{b:02x}").expect("string write");
    }
    s
}
pub(super) fn decimal_type(text: &str) -> Result<String, DbError> {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    let mut parts = digits.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || (integer.is_empty() && fraction.is_empty())
        || !integer
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return Err(DbError::query(
            "DECIMAL parameters require plain decimal digits",
        ));
    }
    let scale = fraction.trim_end_matches('0').len();
    let precision = (integer.trim_start_matches('0').len() + scale).max(1);
    if scale > 30 || precision > 65 {
        return Err(DbError::query(
            "DECIMAL exceeds MySQL precision 65 or scale 30",
        ));
    }
    Ok(format!("DECIMAL({precision},{scale})"))
}
pub(super) fn bound(v: &Value) -> Result<MyValue, DbError> {
    Ok(match v {
        Value::Null => MyValue::NULL,
        Value::Bool(b) => MyValue::Int(i64::from(*b)),
        Value::Int(s) => {
            if let Ok(n) = s.parse::<i64>() {
                MyValue::Int(n)
            } else if let Ok(n) = s.parse::<u64>() {
                MyValue::UInt(n)
            } else {
                return Err(DbError::query(
                    "Integer parameter exceeds MySQL 64-bit range",
                ));
            }
        }
        Value::Float(FloatValue::Number(n)) if n.is_finite() => MyValue::Double(*n),
        Value::Bytes { len, hex } => {
            if len.checked_mul(2) != Some(hex.len() as u64)
                || !hex.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(DbError::query("BLOB parameter requires complete hex bytes"));
            }
            MyValue::Bytes(
                (0..hex.len())
                    .step_by(2)
                    .map(|i| {
                        u8::from_str_radix(&hex[i..i + 2], 16)
                            .map_err(|_| DbError::query("BLOB parameter requires complete hex bytes"))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            )
        }
        Value::Decimal(s) => {
            decimal_type(s)?;
            MyValue::Bytes(s.as_bytes().into())
        }
        Value::Text(s) | Value::Date(s) | Value::Time(s) | Value::DateTime(s) | Value::Uuid(s) => {
            MyValue::Bytes(s.as_bytes().into())
        }
        Value::DateTimeTz(s) => {
            // Protected connections are UTC; execute also checks the session zone before tagged timestamps.
            let utc = s
                .strip_suffix("+00:00")
                .or_else(|| s.strip_suffix('Z'))
                .ok_or_else(|| DbError::query("Timestamp parameters must use UTC (+00:00 or Z)"))?;
            MyValue::Bytes(utc.replace('T', " ").into_bytes())
        }
        Value::Json(s) => {
            let _: Box<serde_json::value::RawValue> =
                serde_json::from_str(s).map_err(|_| DbError::query("Invalid JSON parameter"))?;
            MyValue::Bytes(s.as_bytes().into())
        }
        _ => return Err(DbError::unsupported("Unsupported MySQL parameter value")),
    })
}
pub(super) fn parameter(v: &serde_json::Value) -> Result<MyValue, DbError> {
    if v.is_object() && v.get("t").is_some() {
        return bound(
            &serde_json::from_value::<Value>(v.clone())
                .map_err(|_| DbError::query("Invalid typed parameter"))?,
        );
    }
    match v {
        serde_json::Value::Null => Ok(MyValue::NULL),
        serde_json::Value::Bool(b) => Ok(MyValue::Int(i64::from(*b))),
        serde_json::Value::String(s) => Ok(MyValue::Bytes(s.as_bytes().into())),
        serde_json::Value::Number(n) => {
            if let Some(n) = n.as_i64() {
                Ok(MyValue::Int(n))
            } else if let Some(n) = n.as_u64() {
                Ok(MyValue::UInt(n))
            } else {
                Ok(MyValue::Double(n.as_f64().ok_or_else(|| {
                    DbError::query("Invalid number parameter")
                })?))
            }
        }
        _ => Err(DbError::query(
            "Use scalar JSON or tagged Value parameters; JSON/DECIMAL use tagged strings",
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precision_and_boundary_validation() {
        assert_eq!(
            bound(&Value::Int(u64::MAX.to_string())).unwrap(),
            MyValue::UInt(u64::MAX)
        );
        assert!(bound(&Value::Int("18446744073709551616".into())).is_err());
        assert!(bound(&Value::Bytes {
            len: 65,
            hex: "00".into()
        })
        .is_err());
        let s = "{\"n\":18446744073709551615}";
        assert_eq!(
            bound(&Value::Json(s.into())).unwrap(),
            MyValue::Bytes(s.as_bytes().into())
        );
        assert_eq!(quote("a`;DROP TABLE x"), "`a``;DROP TABLE x`");
    }
    #[test]
    fn old_driver_manifest_without_icon_key_deserializes() {
        let mut value = serde_json::to_value(MysqlDriver::new().manifest()).unwrap();
        assert_eq!(value["icon_key"], "mysql");
        value.as_object_mut().unwrap().remove("icon_key");
        let manifest: DriverManifest = serde_json::from_value(value).unwrap();
        assert!(manifest.icon_key.is_none());
    }
    #[test]
    fn decimal_filters_never_round_to_the_column_scale() {
        assert_eq!(
            decimal_type("9007199254740992.00000000000000000001").unwrap(),
            "DECIMAL(36,20)"
        );
        assert_eq!(decimal_type("1.001").unwrap(), "DECIMAL(4,3)");
        assert!(decimal_type("1e3").is_err());
        assert!(decimal_type("--1").is_err());
    }
}
