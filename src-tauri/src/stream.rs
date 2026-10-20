use dbcore::{CancelToken, Column, DbError, ErrorKind, ResultSet, Table, Value};
use serde::Serialize;
use serde_json::Value as Json;
use tauri::ipc::{Channel, Response};

#[derive(Debug, Serialize)]
pub struct PackedColumn {
    tag: Option<String>,
    values: Vec<Json>,
}
#[derive(Debug, Serialize)]
pub struct PackedTable {
    columns: Vec<Column>,
    count: usize,
    data: Vec<PackedColumn>,
}
pub fn pack(table: Table) -> Result<PackedTable, DbError> {
    if table
        .rows
        .iter()
        .any(|row| row.len() != table.columns.len())
    {
        return Err(DbError::internal("Invalid result row width"));
    }
    let count = table.rows.len();
    let tags: Vec<_> = (0..table.columns.len())
        .map(|index| {
            table
                .rows
                .first()
                .and_then(|row| simple_tag(&row[index]))
                .filter(|tag| {
                    table.rows.iter().all(|row| {
                        matches!(row[index], Value::Null) || simple_tag(&row[index]) == Some(*tag)
                    })
                })
        })
        .collect();
    let mut data: Vec<_> = tags
        .iter()
        .map(|tag| PackedColumn {
            tag: tag.map(str::to_owned),
            values: Vec::with_capacity(count),
        })
        .collect();
    for row in table.rows {
        for ((column, tag), value) in data.iter_mut().zip(&tags).zip(row) {
            column.values.push(if tag.is_some() {
                match value {
                    Value::Null => Json::Null,
                    Value::Bool(v) => Json::Bool(v),
                    Value::Int(v)
                    | Value::Decimal(v)
                    | Value::Text(v)
                    | Value::Date(v)
                    | Value::Time(v)
                    | Value::DateTime(v)
                    | Value::DateTimeTz(v)
                    | Value::Duration(v)
                    | Value::Uuid(v)
                    | Value::Json(v) => Json::String(v),
                    Value::Float(v) => serde_json::to_value(v)
                        .map_err(|_| DbError::internal("Cannot encode result cell"))?,
                    _ => return Err(DbError::internal("Invalid packed result tag")),
                }
            } else {
                serde_json::to_value(value)
                    .map_err(|_| DbError::internal("Cannot encode result cell"))?
            });
        }
    }
    Ok(PackedTable {
        columns: table.columns,
        count,
        data,
    })
}

fn simple_tag(value: &Value) -> Option<&'static str> {
    Some(match value {
        Value::Null => "Null",
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Decimal(_) => "Decimal",
        Value::Text(_) => "Text",
        Value::Date(_) => "Date",
        Value::Time(_) => "Time",
        Value::DateTime(_) => "DateTime",
        Value::DateTimeTz(_) => "DateTimeTz",
        Value::Duration(_) => "Duration",
        Value::Uuid(_) => "Uuid",
        Value::Json(_) => "Json",
        _ => return None,
    })
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StreamEvent {
    Start {
        index: usize,
        result: Box<ResultSet>,
    },
    Rows {
        index: usize,
        offset: usize,
        table: PackedTable,
    },
    End {
        index: usize,
        result: Json,
    },
    Done,
}
fn send(channel: &Channel<Response>, event: StreamEvent) -> Result<(), DbError> {
    let bytes = rmp_serde::to_vec_named(&event)
        .map_err(|_| DbError::internal("Cannot encode result frame"))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(DbError::internal("Result frame too large"));
    }
    channel
        .send(Response::new(bytes))
        .map_err(|_| DbError::new(ErrorKind::Cancelled, "Result receiver closed"))
}
pub async fn send_results(
    results: Vec<ResultSet>,
    channel: &Channel<Response>,
    cancel: &CancelToken,
) -> Result<(), DbError> {
    for (index, mut result) in results.into_iter().enumerate() {
        let table = result.table.take();
        let columns = table.as_ref().map(|t| t.columns.clone());
        let graph = result.graph.take();
        let documents = result.documents.take();
        let mut meta = serde_json::to_value(&result)
            .map_err(|_| DbError::internal("Cannot encode result metadata"))?;
        for key in ["table", "graph", "documents"] {
            meta.as_object_mut().unwrap().remove(key);
        }
        result.table = columns.as_ref().map(|c| Table {
            columns: c.clone(),
            rows: vec![],
        });
        result.graph = graph;
        result.documents = documents;
        result.row_count = 0;
        send(channel, StreamEvent::Start { index, result: Box::new(result) })?;
        if let Some(table) = table {
            let mut rows = table.rows.into_iter();
            let mut offset = 0;
            loop {
                if cancel.is_cancelled() {
                    return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
                }
                let batch: Vec<Vec<Value>> = rows
                    .by_ref()
                    .take(if offset == 0 { 200 } else { 2000 })
                    .collect();
                if batch.is_empty() {
                    break;
                }
                let count = batch.len();
                send(
                    channel,
                    StreamEvent::Rows {
                        index,
                        offset,
                        table: pack(Table {
                            columns: table.columns.clone(),
                            rows: batch,
                        })?,
                    },
                )?;
                offset += count;
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }
        send(
            channel,
            StreamEvent::End {
                index,
                result: meta,
            },
        )?;
    }
    send(channel, StreamEvent::Done)
}
struct CurrentResult {
    index: usize,
    columns: Vec<Column>,
    offset: usize,
    metadata: Json,
    expected: Option<u64>,
    pending: bool,
}

pub struct LiveResults {
    channel: Channel<Response>,
    current: std::sync::Mutex<Option<CurrentResult>>,
    cancel: CancelToken,
    row_limit: usize,
}
impl LiveResults {
    pub fn new(channel: Channel<Response>, cancel: CancelToken, row_limit: u32) -> Self {
        Self {
            channel,
            current: std::sync::Mutex::new(None),
            cancel,
            row_limit: row_limit as usize,
        }
    }
    pub fn started(&self) -> bool {
        self.current.lock().unwrap().is_some()
    }
    pub fn finish(&self) -> Result<(), DbError> {
        if let Some(result) = self.current.lock().unwrap().take() {
            if result.pending || result.expected.is_some_and(|n| n != result.offset as u64) {
                return Err(DbError::internal("Incomplete driver row stream"));
            }
            send(
                &self.channel,
                StreamEvent::End {
                    index: result.index,
                    result: result.metadata,
                },
            )?;
        }
        send(&self.channel, StreamEvent::Done)
    }
}
impl dbcore::ResultSink for LiveResults {
    fn start_pending(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        self.start(index, result)?;
        let mut current = self.current.lock().unwrap();
        let current = current.as_mut().unwrap();
        current.expected = None;
        current.pending = true;
        Ok(())
    }
    fn end(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        let mut current = self.current.lock().unwrap();
        let current = current.as_mut().filter(|r| r.index == index && r.pending && r.offset as u64 == result.row_count).ok_or_else(|| DbError::internal("Invalid final result metadata"))?;
        let mut metadata = serde_json::to_value(result).map_err(|_| DbError::internal("Cannot encode result metadata"))?;
        for key in ["table", "graph", "documents"] { metadata.as_object_mut().unwrap().remove(key); }
        current.metadata = metadata;
        current.pending = false;
        current.expected = Some(result.row_count);
        Ok(())
    }
    fn start(&self, index: usize, result: &ResultSet) -> Result<(), DbError> {
        if self.cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
        }
        let mut current = self.current.lock().unwrap();
        if index != current.as_ref().map_or(0, |r| r.index + 1)
            || result.graph.is_some()
            || result.documents.is_some()
        {
            return Err(DbError::internal("Invalid table stream metadata"));
        }
        if let Some(previous) = current.take() {
            if previous.pending || previous
                .expected
                .is_some_and(|n| n != previous.offset as u64)
            {
                return Err(DbError::internal("Incomplete driver row stream"));
            }
            send(
                &self.channel,
                StreamEvent::End {
                    index: previous.index,
                    result: previous.metadata,
                },
            )?;
        }
        let columns = result
            .table
            .as_ref()
            .map(|t| t.columns.clone())
            .unwrap_or_default();
        let start = ResultSet {
            edit_target: result.edit_target.clone(),
            edit_disabled_reason: result.edit_disabled_reason.clone(),
            title: result.title.clone(),
            table: result.table.as_ref().map(|_| Table {
                columns: columns.clone(),
                rows: vec![],
            }),
            graph: None,
            documents: None,
            message: result.message.clone(),
            affected: result.affected,
            row_count: 0,
            truncated: result.truncated,
            page: result.page.clone(),
            elapsed_ms: result.elapsed_ms,
        };
        let mut metadata = serde_json::to_value(&start)
            .map_err(|_| DbError::internal("Cannot encode result metadata"))?;
        metadata["row_count"] = Json::from(result.row_count);
        for key in ["table", "graph", "documents"] {
            metadata.as_object_mut().unwrap().remove(key);
        }
        send(
            &self.channel,
            StreamEvent::Start {
                index,
                result: Box::new(start),
            },
        )?;
        *current = Some(CurrentResult {
            index,
            columns,
            offset: 0,
            metadata,
            expected: result.table.as_ref().map(|_| result.row_count),
            pending: false,
        });
        Ok(())
    }
    fn rows(&self, index: usize, rows: Vec<Vec<Value>>) -> Result<(), DbError> {
        if self.cancel.is_cancelled() {
            return Err(DbError::new(ErrorKind::Cancelled, "실행이 취소되었습니다"));
        }
        let mut current = self.current.lock().unwrap();
        let result = current
            .as_mut()
            .filter(|r| r.index == index)
            .ok_or_else(|| DbError::internal("Out-of-order driver rows"))?;
        if rows.len() > self.row_limit.saturating_sub(result.offset) {
            return Err(DbError::internal("Driver exceeded result row limit"));
        }
        let count = rows.len();
        send(
            &self.channel,
            StreamEvent::Rows {
                index,
                offset: result.offset,
                table: pack(Table {
                    columns: result.columns.clone(),
                    rows,
                })?,
            },
        )?;
        result.offset += count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbcore::ResultSink;
    #[test]
    fn live_channel_keeps_event_order_and_rejects_incomplete_or_oversized_rows() {
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Json>::new()));
        let received = events.clone();
        let channel = Channel::new(move |body| {
            let tauri::ipc::InvokeResponseBody::Raw(bytes) = body else {
                panic!("expected raw MessagePack event")
            };
            received
                .lock()
                .unwrap()
                .push(rmp_serde::from_slice::<Json>(&bytes).unwrap());
            Ok(())
        });
        let stream = LiveResults::new(channel, CancelToken::new(), 2);
        let result = ResultSet {
            row_count: 2,
            table: Some(Table {
                columns: vec![Column {
                    name: "n".into(),
                    type_name: "int8".into(),
                }],
                rows: vec![],
            }),
            ..Default::default()
        };
        stream.start(0, &result).unwrap();
        stream
            .rows(0, vec![vec![Value::Int("9007199254740993".into())]])
            .unwrap();
        assert_eq!(
            events.lock().unwrap()[1]["kind"],
            "rows",
            "first rows leave before completion"
        );
        assert!(
            stream.start(1, &result).is_err(),
            "an incomplete result cannot finish"
        );
        // The failed transition consumes the previous state, so start a fresh stream for ordering.
        let events2 = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Json>::new()));
        let received = events2.clone();
        let channel = Channel::new(move |body| {
            let tauri::ipc::InvokeResponseBody::Raw(bytes) = body else {
                panic!()
            };
            received
                .lock()
                .unwrap()
                .push(rmp_serde::from_slice::<Json>(&bytes).unwrap());
            Ok(())
        });
        let stream = LiveResults::new(channel, CancelToken::new(), 2);
        for index in 0..2 {
            stream.start(index, &result).unwrap();
            stream
                .rows(index, vec![vec![Value::Int("1".into())], vec![Value::Null]])
                .unwrap();
        }
        assert!(stream.rows(1, vec![vec![Value::Int("3".into())]]).is_err());
        stream.finish().unwrap();
        let events = events2.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .map(|e| e["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["start", "rows", "end", "start", "rows", "end", "done"]
        );
        assert_eq!(events[1]["table"]["data"][0]["tag"], "Int");
        assert_eq!(events[2]["result"]["row_count"], 2);
    }
    #[test]
    fn pending_metadata_requires_matching_end_before_completion() {
        let channel = Channel::new(|_| Ok(()));
        let stream = LiveResults::new(channel, CancelToken::new(), 2);
        let mut result = ResultSet { table: Some(Table { columns: vec![Column { name: "n".into(), type_name: "int8".into() }], rows: vec![] }), ..Default::default() };
        stream.start_pending(0, &result).unwrap();
        stream.rows(0, vec![vec![Value::Int("1".into())]]).unwrap();
        assert!(stream.end(0, &result).is_err());
        result.row_count = 1;
        result.elapsed_ms = 17;
        result.truncated = true;
        stream.end(0, &result).unwrap();
        assert!(stream.end(0, &result).is_err());
        stream.finish().unwrap();
        let stream = LiveResults::new(Channel::new(|_| Ok(())), CancelToken::new(), 2);
        stream.start_pending(0, &result).unwrap();
        assert!(stream.finish().is_err());
    }
    #[test]
    fn packs_exact_and_mixed_cells() {
        let table = Table {
            columns: vec![
                Column {
                    name: "id".into(),
                    type_name: "int8".into(),
                },
                Column {
                    name: "mixed".into(),
                    type_name: "any".into(),
                },
            ],
            rows: vec![
                vec![
                    Value::Int("9007199254740993123".into()),
                    Value::Text("x".into()),
                ],
                vec![Value::Null, Value::Bool(true)],
            ],
        };
        let packed = serde_json::to_value(pack(table).unwrap()).unwrap();
        assert_eq!(packed["data"][0]["tag"], "Int");
        assert_eq!(packed["data"][0]["values"][0], "9007199254740993123");
        assert!(packed["data"][0]["values"][1].is_null());
        assert!(packed["data"][1]["tag"].is_null());
        assert_eq!(packed["data"][1]["values"][1]["t"], "Bool");
    }
}
