use dbcore::{CancelToken, Column, DbError, ErrorKind, ResultSet, Table, Value};
use serde::Serialize;
use serde_json::Value as Json;
use tauri::ipc::Channel;

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
    let count = table.rows.len();
    let mut data: Vec<PackedColumn> = table
        .columns
        .iter()
        .map(|_| PackedColumn {
            tag: None,
            values: Vec::with_capacity(count),
        })
        .collect();
    for row in table.rows {
        if row.len() != data.len() {
            return Err(DbError::internal("Invalid result row width"));
        }
        for (column, value) in data.iter_mut().zip(row) {
            column.values.push(
                serde_json::to_value(value)
                    .map_err(|_| DbError::internal("Cannot encode result cell"))?,
            );
        }
    }
    for column in &mut data {
        let tag = column
            .values
            .first()
            .and_then(|v| v["t"].as_str())
            .map(str::to_owned);
        if tag.as_ref().is_some_and(|tag| {
            column
                .values
                .iter()
                .all(|v| v["t"] == *tag || v["t"] == "Null")
        }) {
            column.tag = tag;
            for value in &mut column.values {
                *value = value.get_mut("v").map(Json::take).unwrap_or(Json::Null);
            }
        }
    }
    Ok(PackedTable {
        columns: table.columns,
        count,
        data,
    })
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StreamEvent {
    Start {
        index: usize,
        result: ResultSet,
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
fn send(channel: &Channel<StreamEvent>, event: StreamEvent) -> Result<(), DbError> {
    channel
        .send(event)
        .map_err(|_| DbError::new(ErrorKind::Cancelled, "Result receiver closed"))
}
pub async fn send_results(
    results: Vec<ResultSet>,
    channel: &Channel<StreamEvent>,
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
        send(channel, StreamEvent::Start { index, result })?;
        if let Some(table) = table {
            let mut rows = table.rows.into_iter();
            let mut offset = 0;
            loop {
                if cancel.is_cancelled() {
                    return Err(DbError::new(ErrorKind::Cancelled, "Execution cancelled"));
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
#[cfg(test)]
mod tests {
    use super::*;
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
