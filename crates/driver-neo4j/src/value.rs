use dbcore::{DbError, FloatValue, GNode, GRel, Graph, Value};
use neo4rs::{BoltMap, BoltNode, BoltType};
use serde::{de::IntoDeserializer, Deserialize};
use std::collections::BTreeMap;

fn decode<'a, T: Deserialize<'a>>(value: &'a BoltType) -> Result<T, DbError> {
    T::deserialize(value.into_deserializer()).map_err(conversion)
}

fn conversion(_: impl std::fmt::Display) -> DbError {
    DbError::query("Cannot decode Bolt value")
}
fn props(map: &BoltMap) -> Result<Vec<(String, Value)>, DbError> {
    let mut result = map
        .value
        .iter()
        .map(|(k, v)| Ok((k.value.clone(), map_value(v)?)))
        .collect::<Result<Vec<_>, DbError>>()?;
    result.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(result)
}
// shortcut: neo4rs negotiates Bolt 4.x and exposes legacy IDs; use element IDs when its row API preserves Bolt 5 IDs.
fn node(n: &BoltNode) -> Result<GNode, DbError> {
    Ok(GNode {
        id: n.id.value.to_string(),
        labels: n
            .labels
            .iter()
            .map(|x| match x {
                BoltType::String(s) => Ok(s.value.clone()),
                _ => Err(DbError::query("Invalid node label")),
            })
            .collect::<Result<_, _>>()?,
        props: props(&n.properties)?,
    })
}
pub fn map_value(value: &BoltType) -> Result<Value, DbError> {
    Ok(match value {
        BoltType::Null(_) => Value::Null,
        BoltType::Boolean(v) => Value::Bool(v.value),
        BoltType::Integer(v) => Value::Int(v.value.to_string()),
        BoltType::Float(v) => Value::Float(if v.value.is_finite() {
            FloatValue::Number(v.value)
        } else {
            FloatValue::Special(
                if v.value.is_nan() {
                    "NaN"
                } else if v.value.is_sign_positive() {
                    "Infinity"
                } else {
                    "-Infinity"
                }
                .into(),
            )
        }),
        BoltType::String(v) => Value::Text(v.value.clone()),
        BoltType::Bytes(v) => Value::Bytes {
            len: v.value.len() as u64,
            hex: v
                .value
                .iter()
                .take(64)
                .map(|b| format!("{b:02x}"))
                .collect(),
        },
        BoltType::List(v) => Value::List(v.iter().map(map_value).collect::<Result<_, _>>()?),
        BoltType::Map(v) => Value::Map(props(v)?),
        BoltType::Node(v) => Value::Node(node(v)?),
        BoltType::Relation(v) => Value::Rel(GRel {
            id: v.id.value.to_string(),
            start: v.start_node_id.value.to_string(),
            end: v.end_node_id.value.to_string(),
            rel_type: v.typ.value.clone(),
            props: props(&v.properties)?,
        }),
        BoltType::UnboundedRelation(_) => {
            return Err(DbError::query(
                "Relationship endpoints missing outside a path",
            ))
        }
        BoltType::Path(v) => {
            let nodes = v.nodes().iter().map(node).collect::<Result<Vec<_>, _>>()?;
            let raw_rels = v.rels();
            let indices = v.indices();
            if indices.len() % 2 != 0 {
                return Err(DbError::query("Invalid path indices"));
            }
            let mut rels = Vec::new();
            let mut current = 0;
            for pair in indices.as_chunks::<2>().0 {
                let rel_index = pair[0].value;
                let next = usize::try_from(pair[1].value).map_err(conversion)?;
                let rel = raw_rels
                    .get(
                        rel_index
                            .unsigned_abs()
                            .checked_sub(1)
                            .ok_or_else(|| DbError::query("Invalid path index"))?
                            as usize,
                    )
                    .ok_or_else(|| DbError::query("Invalid path relationship"))?;
                let a = nodes
                    .get(current)
                    .ok_or_else(|| DbError::query("Invalid path node"))?;
                let b = nodes
                    .get(next)
                    .ok_or_else(|| DbError::query("Invalid path node"))?;
                let (start, end) = if rel_index > 0 { (a, b) } else { (b, a) };
                rels.push(GRel {
                    id: rel.id.value.to_string(),
                    start: start.id.clone(),
                    end: end.id.clone(),
                    rel_type: rel.typ.value.clone(),
                    props: props(&rel.properties)?,
                });
                current = next;
            }
            Value::Path { nodes, rels }
        }
        BoltType::Date(_) => Value::Date(decode::<chrono::NaiveDate>(value)?.to_string()),
        BoltType::LocalTime(_) => Value::Time(decode::<chrono::NaiveTime>(value)?.to_string()),
        BoltType::Time(v) => {
            let (time, offset): (chrono::NaiveTime, chrono::FixedOffset) = v.into();
            Value::Time(format!("{time}{offset}"))
        }
        BoltType::LocalDateTime(_) => Value::DateTime(
            decode::<chrono::NaiveDateTime>(value)?
                .format("%Y-%m-%dT%H:%M:%S%.f")
                .to_string(),
        ),
        BoltType::DateTime(_) => {
            Value::DateTimeTz(decode::<chrono::DateTime<chrono::FixedOffset>>(value)?.to_rfc3339())
        }
        BoltType::DateTimeZoneId(v) => Value::DateTimeTz(format!(
            "{}[{}]",
            decode::<chrono::DateTime<chrono::FixedOffset>>(value)?.to_rfc3339(),
            v.tz_id()
        )),
        BoltType::Duration(_) => {
            let (months, days, seconds, nanos): (i64, i64, i64, i64) = decode(value)?;
            let total = i128::from(seconds) * 1_000_000_000 + i128::from(nanos);
            let sign = if total < 0 { "-" } else { "" };
            let abs = total.abs();
            Value::Duration(format!(
                "P{months}M{days}DT{sign}{}.{:09}S",
                abs / 1_000_000_000,
                abs % 1_000_000_000
            ))
        }
        BoltType::Point2D(v) => Value::Point {
            srid: v.sr_id.value as i32,
            x: v.x.value,
            y: v.y.value,
            z: None,
        },
        BoltType::Point3D(v) => Value::Point {
            srid: v.sr_id.value as i32,
            x: v.x.value,
            y: v.y.value,
            z: Some(v.z.value),
        },
    })
}
pub fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "Null",
        Value::Bool(_) => "Boolean",
        Value::Int(_) => "Integer",
        Value::Float(_) => "Float",
        Value::Text(_) => "String",
        Value::Bytes { .. } => "Bytes",
        Value::List(_) => "List",
        Value::Map(_) => "Map",
        Value::Node(_) => "Node",
        Value::Rel(_) => "Relationship",
        Value::Path { .. } => "Path",
        Value::Date(_) => "Date",
        Value::Time(_) => "Time",
        Value::DateTime(_) => "LocalDateTime",
        Value::DateTimeTz(_) => "DateTime",
        Value::Duration(_) => "Duration",
        Value::Point { .. } => "Point",
        _ => "Other",
    }
}
#[derive(Default)]
pub struct Collector {
    nodes: BTreeMap<String, GNode>,
    rels: BTreeMap<String, GRel>,
    pub truncated: bool,
}
impl Collector {
    pub fn collect(&mut self, value: &Value, limit: usize) {
        match value {
            Value::Node(n) => {
                if self.nodes.contains_key(&n.id) {
                    return;
                }
                if self.nodes.len() < limit {
                    self.nodes.insert(n.id.clone(), n.clone());
                } else {
                    self.truncated = true;
                }
            }
            Value::Rel(r) => {
                self.rels.insert(r.id.clone(), r.clone());
            }
            Value::Path { nodes, rels } => {
                for n in nodes {
                    self.collect(&Value::Node(n.clone()), limit);
                }
                for r in rels {
                    self.collect(&Value::Rel(r.clone()), limit);
                }
            }
            Value::List(items) => {
                for v in items {
                    self.collect(v, limit);
                }
            }
            Value::Map(items) => {
                for (_, v) in items {
                    self.collect(v, limit);
                }
            }
            _ => {}
        }
    }
    pub fn finish(self) -> Option<Graph> {
        let rels = self
            .rels
            .into_values()
            .filter(|r| self.nodes.contains_key(&r.start) && self.nodes.contains_key(&r.end))
            .collect::<Vec<_>>();
        if self.nodes.is_empty() && rels.is_empty() {
            None
        } else {
            Some(Graph {
                nodes: self.nodes.into_values().collect(),
                rels,
            })
        }
    }
}
pub fn parameter(value: &serde_json::Value) -> Result<BoltType, DbError> {
    use serde_json::Value as J;
    Ok(match value {
        J::Null => BoltType::Null(neo4rs::BoltNull),
        J::Bool(v) => (*v).into(),
        J::String(v) => v.as_str().into(),
        J::Number(v) => {
            if let Some(n) = v.as_i64() {
                n.into()
            } else if v.is_u64() {
                return Err(DbError::query(
                    "Integer parameter is outside Bolt i64 range",
                ));
            } else {
                v.as_f64()
                    .ok_or_else(|| DbError::query("Invalid numeric parameter"))?
                    .into()
            }
        }
        J::Array(v) => BoltType::List(
            v.iter()
                .map(parameter)
                .collect::<Result<Vec<_>, _>>()?
                .into(),
        ),
        J::Object(v) if v.get("t").and_then(J::as_str) == Some("Int") => v
            .get("v")
            .and_then(J::as_str)
            .ok_or_else(|| DbError::query("Int parameter requires a string"))?
            .parse::<i64>()
            .map_err(|_| DbError::query("Invalid Int parameter"))?
            .into(),
        J::Object(v) => BoltType::Map(
            v.iter()
                .map(|(k, v)| Ok((k.as_str().into(), parameter(v)?)))
                .collect::<Result<BoltMap, DbError>>()?,
        ),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bytes_special_float_and_graph_limits() {
        assert_eq!(
            map_value(&BoltType::from(vec![0u8, 255])).unwrap(),
            Value::Bytes {
                len: 2,
                hex: "00ff".into()
            }
        );
        assert_eq!(
            map_value(&BoltType::from(f64::INFINITY)).unwrap(),
            Value::Float(FloatValue::Special("Infinity".into()))
        );
        let a = GNode {
            id: "1".into(),
            labels: vec![],
            props: vec![],
        };
        let b = GNode {
            id: "2".into(),
            labels: vec![],
            props: vec![],
        };
        let r = GRel {
            id: "3".into(),
            start: "1".into(),
            end: "2".into(),
            rel_type: "LINK".into(),
            props: vec![],
        };
        let values = Value::Map(vec![(
            "nested".into(),
            Value::List(vec![
                Value::Node(a.clone()),
                Value::Node(a),
                Value::Path {
                    nodes: vec![b],
                    rels: vec![r],
                },
            ]),
        )]);
        let mut collector = Collector::default();
        collector.collect(&values, 2);
        assert!(!collector.truncated);
        let graph = collector.finish().unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.rels.len(), 1);
        let mut collector = Collector::default();
        collector.collect(&values, 0);
        assert!(collector.truncated);
        assert!(collector.finish().is_none());
    }
    #[test]
    fn integer_precision_and_duration() {
        assert_eq!(
            parameter(&serde_json::json!({"t":"Int","v":"9007199254740993"})).unwrap(),
            BoltType::from(9007199254740993i64)
        );
        assert!(parameter(&serde_json::json!(u64::MAX)).is_err());
        assert_eq!(
            map_value(&BoltType::Duration(neo4rs::BoltDuration::new(
                2.into(),
                3.into(),
                4.into(),
                5.into()
            )))
            .unwrap(),
            Value::Duration("P2M3DT4.000000005S".into())
        );
    }
}
