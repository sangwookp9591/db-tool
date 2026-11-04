//! Row transforms compiled against the source columns. Pure and synchronous.
use crate::spec::{CastType, Cmp, Step, TextFunc};
use dbcore::{DbError, FloatValue, Value};
use std::cmp::Ordering;
use std::collections::HashSet;

/// Upper bound on distinct keys a `dedupe` step remembers in one run.
pub const MAX_DEDUPE_KEYS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq)]
pub struct RowError {
    /// 1-based step number.
    pub step: usize,
    pub message: String,
}

#[derive(Debug)]
enum Op {
    Project(Vec<usize>),
    Cast(usize, CastType),
    Filter(usize, Cmp, Option<Value>),
    /// Write a value into an existing column or append a new one.
    Put(Slot, Value),
    Text(usize, TextFunc),
    FillNull(usize, Value),
    Concat(Slot, Vec<usize>, String),
    Dedupe(Vec<usize>),
}
#[derive(Debug, Clone, Copy)]
enum Slot {
    At(usize),
    Append,
}

#[derive(Debug)]
pub struct Transform {
    ops: Vec<Op>,
    output: Vec<String>,
    /// Cast targets that survive to the output, by output position.
    casts: Vec<Option<CastType>>,
    seen: Vec<HashSet<String>>,
}

fn find(names: &[String], name: &str, step: usize) -> Result<usize, DbError> {
    names
        .iter()
        .position(|n| n == name)
        .ok_or_else(|| DbError::query(format!("단계 {step}: 컬럼을 찾을 수 없습니다: {name}")))
}
fn check_new(names: &[String], name: &str, step: usize) -> Result<(), DbError> {
    if name.trim().is_empty() || name.len() > 255 {
        return Err(DbError::query(format!(
            "단계 {step}: 컬럼 이름은 1..255바이트여야 합니다"
        )));
    }
    if names.iter().any(|n| n == name) {
        return Err(DbError::query(format!(
            "단계 {step}: 중복된 컬럼입니다: {name}"
        )));
    }
    Ok(())
}

impl Transform {
    /// Resolve every column reference once so per-row work is index based.
    pub fn compile(input: &[String], steps: &[Step]) -> Result<Self, DbError> {
        let mut names = input.to_vec();
        let mut casts: Vec<Option<CastType>> = vec![None; names.len()];
        let mut ops = Vec::with_capacity(steps.len());
        let mut dedupes = 0;
        for (i, step) in steps.iter().enumerate() {
            let n = i + 1;
            match step {
                Step::Select { columns } => {
                    if columns.is_empty() {
                        return Err(DbError::query(format!(
                            "단계 {n}: 선택할 컬럼이 필요합니다"
                        )));
                    }
                    let mut next = Vec::with_capacity(columns.len());
                    let mut index = Vec::with_capacity(columns.len());
                    for c in columns {
                        index.push(find(&names, &c.from, n)?);
                        check_new(&next, &c.to, n)?;
                        next.push(c.to.clone());
                    }
                    casts = index.iter().map(|&i| casts[i]).collect();
                    names = next;
                    ops.push(Op::Project(index));
                }
                Step::Rename { from, to } => {
                    let at = find(&names, from, n)?;
                    if from != to {
                        check_new(&names, to, n)?;
                    }
                    names[at] = to.clone();
                }
                Step::Drop { columns } => {
                    let mut drop = HashSet::new();
                    for c in columns {
                        drop.insert(find(&names, c, n)?);
                    }
                    let keep: Vec<usize> = (0..names.len()).filter(|i| !drop.contains(i)).collect();
                    if keep.is_empty() {
                        return Err(DbError::query(format!(
                            "단계 {n}: 모든 컬럼을 제거할 수 없습니다"
                        )));
                    }
                    names = keep.iter().map(|&i| names[i].clone()).collect();
                    casts = keep.iter().map(|&i| casts[i]).collect();
                    ops.push(Op::Project(keep));
                }
                Step::Cast { column, to } => {
                    let at = find(&names, column, n)?;
                    casts[at] = Some(*to);
                    ops.push(Op::Cast(at, *to));
                }
                Step::Filter { column, cmp, value } => {
                    ops.push(Op::Filter(find(&names, column, n)?, *cmp, value.clone()));
                }
                Step::Constant { column, value } => {
                    let slot = slot(&mut names, &mut casts, column, n)?;
                    ops.push(Op::Put(slot, value.clone()));
                }
                Step::Text { column, func } => ops.push(Op::Text(find(&names, column, n)?, *func)),
                Step::FillNull { column, value } => {
                    ops.push(Op::FillNull(find(&names, column, n)?, value.clone()))
                }
                Step::Concat {
                    column,
                    inputs,
                    separator,
                } => {
                    if inputs.is_empty() {
                        return Err(DbError::query(format!("단계 {n}: 합칠 컬럼이 필요합니다")));
                    }
                    let index = inputs
                        .iter()
                        .map(|c| find(&names, c, n))
                        .collect::<Result<Vec<_>, _>>()?;
                    let slot = slot(&mut names, &mut casts, column, n)?;
                    if let Slot::At(at) = slot {
                        casts[at] = None;
                    }
                    ops.push(Op::Concat(slot, index, separator.clone()));
                }
                Step::Dedupe { columns } => {
                    if columns.is_empty() {
                        return Err(DbError::query(format!(
                            "단계 {n}: 중복 판단 컬럼이 필요합니다"
                        )));
                    }
                    let index = columns
                        .iter()
                        .map(|c| find(&names, c, n))
                        .collect::<Result<Vec<_>, _>>()?;
                    dedupes += 1;
                    ops.push(Op::Dedupe(index));
                }
            }
        }
        Ok(Self {
            ops,
            output: names,
            casts,
            seen: (0..dedupes).map(|_| HashSet::new()).collect(),
        })
    }
    pub fn output(&self) -> &[String] {
        &self.output
    }
    /// Explicit cast per output column, used to type `create_table` columns.
    pub fn output_casts(&self) -> &[Option<CastType>] {
        &self.casts
    }
    /// `Ok(None)` means the row was filtered out or deduplicated.
    pub fn apply(&mut self, mut row: Vec<Value>) -> Result<Option<Vec<Value>>, RowError> {
        let mut dedupe = 0;
        for (i, op) in self.ops.iter().enumerate() {
            let err = |message: String| RowError {
                step: i + 1,
                message,
            };
            match op {
                Op::Project(index) => {
                    let mut taken: Vec<Option<Value>> = row.into_iter().map(Some).collect();
                    let mut next: Vec<Value> = Vec::with_capacity(index.len());
                    for &at in index {
                        // A column may be selected twice under different names.
                        let value = match taken.get_mut(at).and_then(Option::take) {
                            Some(v) => v,
                            None => next
                                .iter()
                                .zip(index.iter())
                                .find(|(_, src)| **src == at)
                                .map(|(v, _)| v.clone())
                                .unwrap_or(Value::Null),
                        };
                        next.push(value);
                    }
                    row = next;
                }
                Op::Cast(at, to) => {
                    let value = std::mem::replace(&mut row[*at], Value::Null);
                    row[*at] = cast(value, *to).map_err(err)?;
                }
                Op::Filter(at, cmp, value) => {
                    if !matches(&row[*at], *cmp, value.as_ref()) {
                        return Ok(None);
                    }
                }
                Op::Put(slot, value) => put(&mut row, *slot, value.clone()),
                Op::Text(at, func) => {
                    if let Some(text) = scalar_text(&row[*at]).map_err(err)? {
                        row[*at] = Value::Text(match func {
                            TextFunc::Trim => text.trim().to_owned(),
                            TextFunc::Upper => text.to_uppercase(),
                            TextFunc::Lower => text.to_lowercase(),
                        });
                    }
                }
                Op::FillNull(at, value) => {
                    if row[*at] == Value::Null {
                        row[*at] = value.clone();
                    }
                }
                Op::Concat(slot, inputs, separator) => {
                    let mut parts = Vec::with_capacity(inputs.len());
                    for &at in inputs {
                        if let Some(text) = scalar_text(&row[at]).map_err(err)? {
                            parts.push(text);
                        }
                    }
                    put(&mut row, *slot, Value::Text(parts.join(separator)));
                }
                Op::Dedupe(index) => {
                    let key: Vec<&Value> = index.iter().map(|&at| &row[at]).collect();
                    let key = serde_json::to_string(&key)
                        .map_err(|_| err("중복 판단 키를 만들 수 없습니다".into()))?;
                    let seen = &mut self.seen[dedupe];
                    dedupe += 1;
                    if seen.contains(&key) {
                        return Ok(None);
                    }
                    if seen.len() >= MAX_DEDUPE_KEYS {
                        return Err(err(format!(
                            "중복 제거 키가 {MAX_DEDUPE_KEYS}개를 넘었습니다"
                        )));
                    }
                    seen.insert(key);
                }
            }
        }
        Ok(Some(row))
    }
}
fn slot(
    names: &mut Vec<String>,
    casts: &mut Vec<Option<CastType>>,
    column: &str,
    step: usize,
) -> Result<Slot, DbError> {
    if let Some(at) = names.iter().position(|n| n == column) {
        return Ok(Slot::At(at));
    }
    check_new(names, column, step)?;
    names.push(column.to_owned());
    casts.push(None);
    Ok(Slot::Append)
}
fn put(row: &mut Vec<Value>, slot: Slot, value: Value) {
    match slot {
        Slot::At(at) => row[at] = value,
        Slot::Append => row.push(value),
    }
}

fn scalar_text(value: &Value) -> Result<Option<String>, String> {
    dbcore::edits::text(value).map_err(|e| e.message)
}

fn is_integer(s: &str) -> bool {
    let digits = s.strip_prefix(['-', '+']).unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}
fn is_decimal(s: &str) -> bool {
    let body = s.strip_prefix(['-', '+']).unwrap_or(s);
    let (mantissa, exponent) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(e)),
        None => (body, None),
    };
    let (whole, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
    !(whole.is_empty() && frac.is_empty())
        && digits(whole)
        && digits(frac)
        && exponent.is_none_or(is_integer)
}
fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}
fn is_datetime(s: &str) -> bool {
    s.len() >= 16
        && s.is_char_boundary(10)
        && is_date(&s[..10])
        && matches!(s.as_bytes()[10], b'T' | b' ')
        && s.as_bytes()[13] == b':'
}

/// Convert a value; NULL stays NULL. Failures name the value type, not the value.
pub fn cast(value: Value, to: CastType) -> Result<Value, String> {
    if value == Value::Null {
        return Ok(Value::Null);
    }
    let fail = |v: &Value| {
        Err(format!(
            "{} 값을 {:?}(으)로 변환할 수 없습니다",
            kind(v),
            to
        ))
    };
    match to {
        CastType::Text => match &value {
            Value::Text(_) => Ok(value),
            _ => match scalar_text(&value) {
                Ok(Some(s)) => Ok(Value::Text(s)),
                _ => fail(&value),
            },
        },
        CastType::Int => match &value {
            Value::Int(_) => Ok(value),
            Value::Bool(b) => Ok(Value::Int(if *b { "1" } else { "0" }.into())),
            Value::Float(FloatValue::Number(f)) if f.fract() == 0.0 && f.abs() < 9.0e15 => {
                Ok(Value::Int(format!("{}", *f as i64)))
            }
            Value::Text(s) | Value::Decimal(s) => {
                let t = s.trim();
                let t = t.strip_suffix(".0").unwrap_or(t);
                if is_integer(t) {
                    Ok(Value::Int(t.trim_start_matches('+').to_owned()))
                } else {
                    fail(&value)
                }
            }
            _ => fail(&value),
        },
        CastType::Float => match &value {
            Value::Float(_) => Ok(value),
            Value::Int(s) | Value::Decimal(s) | Value::Text(s) => match s.trim().parse::<f64>() {
                Ok(f) if f.is_finite() => Ok(Value::Float(FloatValue::Number(f))),
                _ => fail(&value),
            },
            Value::Bool(b) => Ok(Value::Float(FloatValue::Number(if *b { 1.0 } else { 0.0 }))),
            _ => fail(&value),
        },
        CastType::Decimal => match &value {
            Value::Decimal(_) => Ok(value),
            Value::Int(s) => Ok(Value::Decimal(s.clone())),
            Value::Float(FloatValue::Number(f)) => Ok(Value::Decimal(f.to_string())),
            Value::Text(s) if is_decimal(s.trim()) => Ok(Value::Decimal(s.trim().to_owned())),
            _ => fail(&value),
        },
        CastType::Bool => match &value {
            Value::Bool(_) => Ok(value),
            Value::Int(s) if s == "0" || s == "1" => Ok(Value::Bool(s == "1")),
            Value::Text(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "t" | "1" | "yes" | "y" => Ok(Value::Bool(true)),
                "false" | "f" | "0" | "no" | "n" => Ok(Value::Bool(false)),
                _ => fail(&value),
            },
            _ => fail(&value),
        },
        CastType::Date => match &value {
            Value::Date(_) => Ok(value),
            Value::DateTime(s) | Value::DateTimeTz(s)
                if s.len() >= 10 && s.is_char_boundary(10) && is_date(&s[..10]) =>
            {
                Ok(Value::Date(s[..10].to_owned()))
            }
            Value::Text(s) if is_date(s.trim()) => Ok(Value::Date(s.trim().to_owned())),
            Value::Text(s) if is_datetime(s.trim()) => Ok(Value::Date(s.trim()[..10].to_owned())),
            _ => fail(&value),
        },
        CastType::DateTime => match &value {
            Value::DateTime(_) | Value::DateTimeTz(_) => Ok(value),
            Value::Date(s) => Ok(Value::DateTime(format!("{s}T00:00:00"))),
            Value::Text(s) if is_datetime(s.trim()) => Ok(Value::DateTime(s.trim().to_owned())),
            Value::Text(s) if is_date(s.trim()) => {
                Ok(Value::DateTime(format!("{}T00:00:00", s.trim())))
            }
            _ => fail(&value),
        },
        CastType::Json => match &value {
            Value::Json(_) => Ok(value),
            Value::Text(s) => match serde_json::from_str::<serde::de::IgnoredAny>(s) {
                Ok(_) => Ok(Value::Json(s.clone())),
                Err(_) => fail(&value),
            },
            Value::Int(s) | Value::Decimal(s) => Ok(Value::Json(s.clone())),
            Value::Bool(b) => Ok(Value::Json(b.to_string())),
            Value::Float(FloatValue::Number(f)) => Ok(Value::Json(f.to_string())),
            _ => fail(&value),
        },
    }
}
fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "Null",
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Decimal(_) => "Decimal",
        Value::Text(_) => "Text",
        Value::Bytes { .. } => "Bytes",
        Value::Date(_) => "Date",
        Value::Time(_) => "Time",
        Value::DateTime(_) => "DateTime",
        Value::DateTimeTz(_) => "DateTimeTz",
        Value::Duration(_) => "Duration",
        Value::Uuid(_) => "Uuid",
        Value::Json(_) => "Json",
        Value::List(_) => "List",
        Value::Map(_) => "Map",
        _ => "Other",
    }
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(s) | Value::Decimal(s) => s.trim().parse().ok(),
        Value::Float(FloatValue::Number(f)) => Some(*f),
        Value::Text(s) if is_decimal(s.trim()) => s.trim().parse().ok(),
        _ => None,
    }
}
fn is_numeric(value: &Value) -> bool {
    matches!(
        value,
        Value::Int(_) | Value::Decimal(_) | Value::Float(FloatValue::Number(_))
    )
}
/// SQL-like ordering: NULL compares as unknown (`None`).
fn compare(left: &Value, right: &Value) -> Option<Ordering> {
    if *left == Value::Null || *right == Value::Null {
        return None;
    }
    if let (Value::Int(a), Value::Int(b)) = (left, right)
        && let (Ok(a), Ok(b)) = (a.parse::<i128>(), b.parse::<i128>())
    {
        return Some(a.cmp(&b));
    }
    if is_numeric(left) || is_numeric(right) {
        return number(left)?.partial_cmp(&number(right)?);
    }
    if let (Value::Bool(a), Value::Bool(b)) = (left, right) {
        return Some(a.cmp(b));
    }
    Some(scalar_text(left).ok()??.cmp(&scalar_text(right).ok()??))
}
pub fn matches(cell: &Value, cmp: Cmp, value: Option<&Value>) -> bool {
    let null = *cell == Value::Null;
    match cmp {
        Cmp::IsNull => return null,
        Cmp::NotNull => return !null,
        _ => {}
    }
    let Some(value) = value else { return false };
    match cmp {
        Cmp::Contains | Cmp::StartsWith => {
            match (
                scalar_text(cell).ok().flatten(),
                scalar_text(value).ok().flatten(),
            ) {
                (Some(a), Some(b)) => {
                    if cmp == Cmp::Contains {
                        a.contains(&b)
                    } else {
                        a.starts_with(&b)
                    }
                }
                _ => false,
            }
        }
        _ => compare(cell, value).is_some_and(|o| match cmp {
            Cmp::Eq => o == Ordering::Equal,
            Cmp::Ne => o != Ordering::Equal,
            Cmp::Lt => o == Ordering::Less,
            Cmp::Le => o != Ordering::Greater,
            Cmp::Gt => o == Ordering::Greater,
            Cmp::Ge => o != Ordering::Less,
            _ => false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::ColumnMap;
    fn int(v: i64) -> Value {
        Value::Int(v.to_string())
    }
    fn text(v: &str) -> Value {
        Value::Text(v.into())
    }
    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_owned()).collect()
    }
    #[test]
    fn pipeline_of_steps_projects_filters_and_derives() {
        let steps = vec![
            Step::Filter {
                column: "age".into(),
                cmp: Cmp::Ge,
                value: Some(int(18)),
            },
            Step::Text {
                column: "name".into(),
                func: TextFunc::Trim,
            },
            Step::Concat {
                column: "label".into(),
                inputs: vec!["name".into(), "city".into()],
                separator: "@".into(),
            },
            Step::Select {
                columns: vec![
                    ColumnMap {
                        from: "id".into(),
                        to: "user_id".into(),
                    },
                    ColumnMap {
                        from: "label".into(),
                        to: "label".into(),
                    },
                ],
            },
            Step::Constant {
                column: "source".into(),
                value: text("crm"),
            },
        ];
        let mut t = Transform::compile(&cols(&["id", "name", "age", "city"]), &steps).unwrap();
        assert_eq!(t.output(), &cols(&["user_id", "label", "source"])[..]);
        assert_eq!(
            t.apply(vec![int(1), text(" Kim "), int(17), text("Seoul")])
                .unwrap(),
            None
        );
        assert_eq!(
            t.apply(vec![int(2), text(" Lee "), int(30), Value::Null])
                .unwrap(),
            Some(vec![int(2), text("Lee"), text("crm")])
        );
    }
    #[test]
    fn unknown_columns_fail_at_compile_time() {
        let err = Transform::compile(
            &cols(&["a"]),
            &[Step::Drop {
                columns: vec!["b".into()],
            }],
        )
        .unwrap_err();
        assert!(err.message.contains("단계 1"));
        assert!(
            Transform::compile(
                &cols(&["a", "b"]),
                &[Step::Rename {
                    from: "a".into(),
                    to: "b".into()
                }]
            )
            .is_err()
        );
        assert!(
            Transform::compile(
                &cols(&["a"]),
                &[Step::Drop {
                    columns: vec!["a".into()]
                }]
            )
            .is_err()
        );
    }
    #[test]
    fn casts_are_strict_and_keep_null() {
        assert_eq!(cast(text(" 42 "), CastType::Int).unwrap(), int(42));
        assert_eq!(cast(text("4.0"), CastType::Int).unwrap(), int(4));
        assert!(cast(text("4.5"), CastType::Int).is_err());
        assert_eq!(cast(Value::Null, CastType::Int).unwrap(), Value::Null);
        assert_eq!(
            cast(text("yes"), CastType::Bool).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            cast(text("2026-11-04 10:00:00"), CastType::Date).unwrap(),
            Value::Date("2026-11-04".into())
        );
        assert_eq!(
            cast(Value::Date("2026-11-04".into()), CastType::DateTime).unwrap(),
            Value::DateTime("2026-11-04T00:00:00".into())
        );
        assert!(cast(text("{bad"), CastType::Json).is_err());
        assert_eq!(
            cast(text("1.5e3"), CastType::Decimal).unwrap(),
            Value::Decimal("1.5e3".into())
        );
        assert!(cast(text("1e"), CastType::Decimal).is_err());
        assert_eq!(cast(int(7), CastType::Text).unwrap(), text("7"));
    }
    #[test]
    fn cast_error_reports_step_and_dedupe_drops_repeats() {
        let steps = vec![
            Step::Cast {
                column: "n".into(),
                to: CastType::Int,
            },
            Step::Dedupe {
                columns: vec!["n".into()],
            },
        ];
        let mut t = Transform::compile(&cols(&["n"]), &steps).unwrap();
        assert_eq!(t.output_casts(), &[Some(CastType::Int)]);
        assert_eq!(t.apply(vec![text("x")]).unwrap_err().step, 1);
        assert!(t.apply(vec![text("1")]).unwrap().is_some());
        assert!(t.apply(vec![int(1)]).unwrap().is_none());
    }
    #[test]
    fn comparisons_follow_types_and_null_is_unknown() {
        assert!(matches(&int(10), Cmp::Gt, Some(&int(9))));
        assert!(matches(
            &Value::Decimal("10.5".into()),
            Cmp::Gt,
            Some(&int(10))
        ));
        assert!(matches(&text("b"), Cmp::Gt, Some(&text("a"))));
        assert!(!matches(&Value::Null, Cmp::Ne, Some(&int(1))));
        assert!(matches(&Value::Null, Cmp::IsNull, None));
        assert!(matches(&text("hello"), Cmp::Contains, Some(&text("ell"))));
        assert!(matches(&text("hello"), Cmp::StartsWith, Some(&text("he"))));
        assert!(matches(
            &int(12345678901234567),
            Cmp::Lt,
            Some(&int(12345678901234568))
        ));
    }
    #[test]
    fn select_can_duplicate_a_column() {
        let steps = vec![Step::Select {
            columns: vec![
                ColumnMap {
                    from: "a".into(),
                    to: "x".into(),
                },
                ColumnMap {
                    from: "a".into(),
                    to: "y".into(),
                },
            ],
        }];
        let mut t = Transform::compile(&cols(&["a", "b"]), &steps).unwrap();
        assert_eq!(
            t.apply(vec![int(1), int(2)]).unwrap(),
            Some(vec![int(1), int(1)])
        );
    }
    #[test]
    fn column_bookkeeping_follows_rename_drop_and_overwrite() {
        let steps = vec![
            Step::Cast {
                column: "b".into(),
                to: CastType::Bool,
            },
            Step::Rename {
                from: "a".into(),
                to: "id".into(),
            },
            Step::Drop {
                columns: vec!["c".into()],
            },
            Step::FillNull {
                column: "id".into(),
                value: int(0),
            },
            Step::Constant {
                column: "b".into(),
                value: Value::Bool(false),
            },
            Step::Rename {
                from: "id".into(),
                to: "id".into(),
            },
        ];
        let mut t = Transform::compile(&cols(&["a", "b", "c"]), &steps).unwrap();
        assert_eq!(t.output(), &cols(&["id", "b"])[..]);
        // The cast on `b` survives the drop at its new position.
        assert_eq!(t.output_casts(), &[None, Some(CastType::Bool)]);
        assert_eq!(
            t.apply(vec![Value::Null, text("yes"), int(9)]).unwrap(),
            Some(vec![int(0), Value::Bool(false)])
        );
        // A step after a rename cannot use the old name.
        let stale = [
            Step::Rename {
                from: "a".into(),
                to: "x".into(),
            },
            Step::Text {
                column: "a".into(),
                func: TextFunc::Upper,
            },
        ];
        assert!(Transform::compile(&cols(&["a"]), &stale).is_err());
    }
    #[test]
    fn null_and_non_scalar_inputs() {
        let steps = vec![
            Step::Text {
                column: "a".into(),
                func: TextFunc::Upper,
            },
            Step::Concat {
                column: "j".into(),
                inputs: vec!["a".into(), "b".into()],
                separator: "-".into(),
            },
        ];
        let mut t = Transform::compile(&cols(&["a", "b"]), &steps).unwrap();
        assert_eq!(
            t.apply(vec![Value::Null, Value::Null]).unwrap(),
            Some(vec![Value::Null, Value::Null, text("")])
        );
        let list = Value::List(vec![int(1)]);
        assert_eq!(
            t.apply(vec![list.clone(), Value::Null]).unwrap_err().step,
            1
        );
        assert!(!matches(&Value::Null, Cmp::Contains, Some(&text("a"))));
        assert!(!matches(&text("abc"), Cmp::Eq, Some(&int(1))));
        assert!(!matches(&list, Cmp::Eq, Some(&list)));
        assert!(!matches(&text("a"), Cmp::Eq, None));
    }
    #[test]
    fn each_dedupe_step_has_its_own_key_set() {
        let steps = vec![
            Step::Dedupe {
                columns: vec!["a".into()],
            },
            Step::Dedupe {
                columns: vec!["b".into()],
            },
        ];
        let mut t = Transform::compile(&cols(&["a", "b"]), &steps).unwrap();
        assert!(t.apply(vec![int(1), int(1)]).unwrap().is_some());
        assert!(t.apply(vec![int(2), int(1)]).unwrap().is_none());
        // `a`=1 was seen by the first step; the second never saw b=2.
        assert!(t.apply(vec![int(1), int(2)]).unwrap().is_none());
        assert!(t.apply(vec![int(3), int(2)]).unwrap().is_some());
        let empty = [Step::Dedupe { columns: vec![] }];
        assert!(Transform::compile(&cols(&["a"]), &empty).is_err());
    }
}
