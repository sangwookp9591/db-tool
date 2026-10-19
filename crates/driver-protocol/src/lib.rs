//! Protocol 1: big-endian u32 length followed by a MessagePack map.
use dbcore::*;
use serde::{Deserialize, Serialize};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const PROTOCOL: u32 = 1;
pub const MAX_FRAME: usize = 16 * 1024 * 1024;
pub const CHUNK_ITEMS: usize = 2000;

#[derive(Serialize)]
#[serde(untagged)]
pub enum Message {
    Request {
        id: u64,
        #[serde(flatten)]
        request: Request,
    },
    Result {
        id: u64,
        result: Reply,
    },
    Error {
        id: u64,
        error: DbError,
    },
    Chunk {
        id: u64,
        chunk: ExecuteChunk,
    },
    Cancel {
        cancel: u64,
    },
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Decode response payloads directly instead of cloning cells while probing untagged variants.
        #[derive(Deserialize)]
        struct Envelope {
            id: Option<u64>,
            result: Option<Reply>,
            error: Option<DbError>,
            chunk: Option<ExecuteChunk>,
            cancel: Option<u64>,
            #[serde(flatten)]
            request: serde_json::Map<String, serde_json::Value>,
        }
        let e = Envelope::deserialize(deserializer)?;
        let request = if e.request.is_empty() {
            None
        } else {
            let value = serde_json::Value::Object(e.request);
            match Request::deserialize(&value) {
                Ok(request) => Some(request),
                Err(_) => {
                    // Unknown optional methods may carry arbitrary params; known malformed methods still fail.
                    let tag = serde_json::json!({"method":value["method"]});
                    match Request::deserialize(&tag) {
                        Ok(Request::Unsupported) => Some(Request::Unsupported),
                        _ => return Err(serde::de::Error::custom("Invalid driver request")),
                    }
                }
            }
        };
        match (e.id, e.result, e.error, e.chunk, e.cancel, request) {
            (Some(id), Some(result), None, None, None, None) => Ok(Self::Result { id, result }),
            (Some(id), None, Some(error), None, None, None) => Ok(Self::Error { id, error }),
            (Some(id), None, None, Some(chunk), None, None) => Ok(Self::Chunk { id, chunk }),
            (Some(id), None, None, None, None, Some(request)) => Ok(Self::Request { id, request }),
            (None, None, None, None, Some(cancel), None) => Ok(Self::Cancel { cancel }),
            _ => Err(serde::de::Error::custom("Invalid driver envelope")),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Handshake {
        protocol: u32,
    },
    Connect(ConnectConfig),
    Ping {
        session: u64,
    },
    ServerInfo {
        session: u64,
    },
    ListChildren {
        session: u64,
        parent: Option<String>,
    },
    Browse {
        session: u64,
        node_id: String,
        page: PageRequest,
    },
    Execute {
        session: u64,
        req: ExecRequest,
    },
    Expand {
        session: u64,
        element_id: String,
        limit: u32,
    },
    SchemaGraph {
        session: u64,
        scope: Option<String>,
    },
    Explain {
        session: u64,
        query: String,
        analyze: bool,
    },
    Preview {
        session: u64,
        req: ExecRequest,
    },
    PreviewEdits {
        session: u64,
        req: EditRequest,
    },
    ApplyEdits {
        session: u64,
        req: EditRequest,
    },
    ApplyMigration {session:u64,req:ExecRequest},
    SchemaSnapshot { session: u64, scope: Option<String> },
    DdlRender { session: u64, diff: SnapshotDiff },
    ImportRowsProgress { session: u64, req: EditRequest, preview: bool },
    ImportRows { session: u64, req: EditRequest, preview: bool },
    Backup { session: u64 },
    Restore { session: u64, backup: BackupData, overwrite: bool },
    PreviewGrants {session:u64,changes:Vec<GrantChange>},
    ApplyGrants { session:u64, changes:Vec<GrantChange> },
    Roles { session: u64 },
    PlanGrants { session: u64, changes: Vec<GrantChange> },
    Close {
        session: u64,
    },
    #[serde(other)]
    Unsupported,
}

impl Request {
    pub fn session(&self) -> Option<u64> {
        match self {
            Self::Handshake { .. } | Self::Connect(_) | Self::Unsupported => None,
            Self::Ping { session }
            | Self::ServerInfo { session }
            | Self::ListChildren { session, .. }
            | Self::Browse { session, .. }
            | Self::Execute { session, .. }
            | Self::Expand { session, .. }
            | Self::SchemaGraph { session, .. }
            | Self::Explain { session, .. }
            | Self::Preview { session, .. }
            | Self::PreviewEdits { session, .. }
            | Self::ApplyEdits { session, .. }
            | Self::ApplyMigration {session,..}
            | Self::SchemaSnapshot {session,..}
            | Self::DdlRender {session,..}
            | Self::ImportRowsProgress {session,..}
            | Self::ImportRows {session,..}
            | Self::Backup {session}
            | Self::Restore {session,..}
            | Self::PreviewGrants {session,..}
            | Self::ApplyGrants {session,..}
            | Self::Roles {session}
            | Self::PlanGrants {session,..}
            | Self::Close { session } => Some(*session),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Reply {
    Handshake {
        protocol: u32,
        manifest: DriverManifest,
    },
    Connected {
        session: u64,
    },
    Unit,
    Info(String),
    Children(Vec<ExplorerNode>),
    ResultSet(ResultSet),
    SchemaGraph(SchemaGraph),
    Plan(PlanTree),
    Preview(ChangePreview),
    Edits(EditResult),
    Snapshot(SchemaSnapshot),
    Ddl(Vec<String>),
    Import(ImportOutput),
    Migration(Vec<ResultSet>),
    Backup(BackupData),
    Roles(RolesSnapshot),
    Grants(GrantPlan),
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecuteChunk {
    Start {
        index: usize,
        result: Box<ResultSet>,
        #[serde(default)]
        streaming: bool,
    },
    Rows {
        index: usize,
        rows: Vec<Vec<Value>>,
    },
    Documents {
        index: usize,
        documents: Vec<Value>,
    },
    Nodes {
        index: usize,
        nodes: Vec<GNode>,
    },
    Relationships {
        index: usize,
        rels: Vec<GRel>,
    },
}

impl<'de> Deserialize<'de> for ExecuteChunk {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Kind {
            Start,
            Rows,
            Documents,
            Nodes,
            Relationships,
        }
        #[derive(Deserialize)]
        struct Envelope {
            #[serde(rename = "type")]
            kind: Kind,
            index: usize,
            result: Option<Box<ResultSet>>,
            rows: Option<Vec<Vec<Value>>>,
            documents: Option<Vec<Value>>,
            nodes: Option<Vec<GNode>>,
            rels: Option<Vec<GRel>>,
            #[serde(default)]
            streaming: bool,
        }
        let e = Envelope::deserialize(deserializer)?;
        match (e.kind, e.result, e.rows, e.documents, e.nodes, e.rels) {
            (Kind::Start, Some(result), None, None, None, None) => Ok(Self::Start {
                index: e.index,
                result,
                streaming: e.streaming,
            }),
            (Kind::Rows, None, Some(rows), None, None, None) => Ok(Self::Rows {
                index: e.index,
                rows,
            }),
            (Kind::Documents, None, None, Some(documents), None, None) => Ok(Self::Documents {
                index: e.index,
                documents,
            }),
            (Kind::Nodes, None, None, None, Some(nodes), None) => Ok(Self::Nodes {
                index: e.index,
                nodes,
            }),
            (Kind::Relationships, None, None, None, None, Some(rels)) => Ok(Self::Relationships {
                index: e.index,
                rels,
            }),
            _ => Err(serde::de::Error::custom("Invalid execute chunk")),
        }
    }
}

impl ExecuteChunk {
    pub fn split(self) -> Option<(Self, Self)> {
        fn halves<T>(mut values: Vec<T>) -> Option<(Vec<T>, Vec<T>)> {
            if values.len() < 2 {
                return None;
            }
            let rest = values.split_off(values.len() / 2);
            Some((values, rest))
        }
        Some(match self {
            Self::Rows { index, rows } => {
                let (a, b) = halves(rows)?;
                (Self::Rows { index, rows: a }, Self::Rows { index, rows: b })
            }
            Self::Documents { index, documents } => {
                let (a, b) = halves(documents)?;
                (
                    Self::Documents {
                        index,
                        documents: a,
                    },
                    Self::Documents {
                        index,
                        documents: b,
                    },
                )
            }
            Self::Nodes { index, nodes } => {
                let (a, b) = halves(nodes)?;
                (
                    Self::Nodes { index, nodes: a },
                    Self::Nodes { index, nodes: b },
                )
            }
            Self::Relationships { index, rels } => {
                let (a, b) = halves(rels)?;
                (
                    Self::Relationships { index, rels: a },
                    Self::Relationships { index, rels: b },
                )
            }
            Self::Start { .. } => return None,
        })
    }

    pub fn append(self, results: &mut Vec<ResultSet>) -> Result<(), DbError> {
        let invalid = || DbError::internal("Invalid driver result chunk sequence");
        match self {
            Self::Start { index, result, .. } if index == results.len() => results.push(*result),
            Self::Rows { index, rows } => results
                .get_mut(index)
                .and_then(|r| r.table.as_mut())
                .ok_or_else(invalid)?
                .rows
                .extend(rows),
            Self::Documents { index, documents } => results
                .get_mut(index)
                .and_then(|r| r.documents.as_mut())
                .ok_or_else(invalid)?
                .extend(documents),
            Self::Nodes { index, nodes } => results
                .get_mut(index)
                .and_then(|r| r.graph.as_mut())
                .ok_or_else(invalid)?
                .nodes
                .extend(nodes),
            Self::Relationships { index, rels } => results
                .get_mut(index)
                .and_then(|r| r.graph.as_mut())
                .ok_or_else(invalid)?
                .rels
                .extend(rels),
            _ => return Err(invalid()),
        }
        Ok(())
    }
}

pub fn encode(message: &Message) -> io::Result<Vec<u8>> {
    let bytes = rmp_serde::to_vec_named(message).map_err(io::Error::other)?;
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Driver frame exceeds 16 MiB",
        ));
    }
    Ok(bytes)
}

pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &Message,
) -> io::Result<()> {
    let bytes = encode(message)?;
    write_encoded_frame(writer, &bytes).await
}

pub async fn write_encoded_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid driver frame length",
        ));
    }
    writer.write_u32(bytes.len() as u32).await?;
    writer.write_all(bytes).await?;
    writer.flush().await
}

pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Option<Message>> {
    Ok(read_frame_with_size(reader)
        .await?
        .map(|(message, _)| message))
}

pub async fn read_frame_with_size<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> io::Result<Option<(Message, usize)>> {
    let mut length = [0; 4];
    if reader.read(&mut length[..1]).await? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut length[1..]).await?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid driver frame length",
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    rmp_serde::from_slice(&bytes)
        .map(|message| Some((message, length)))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn unknown_optional_methods_decode_without_closing_transport() {
        #[derive(Serialize)]
        struct FutureRequest {
            id: u64,
            method: &'static str,
            params: std::collections::BTreeMap<&'static str, u64>,
        }
        let bytes = rmp_serde::to_vec_named(&FutureRequest {
            id: 9,
            method: "future_optional_method",
            params: std::collections::BTreeMap::from([("session", 1)]),
        }).unwrap();
        assert!(matches!(rmp_serde::from_slice::<Message>(&bytes).unwrap(), Message::Request { id: 9, request: Request::Unsupported }));
        let malformed = rmp_serde::to_vec_named(&FutureRequest {
            id: 10,
            method: "explain",
            params: std::collections::BTreeMap::from([("session", 1)]),
        }).unwrap();
        assert!(rmp_serde::from_slice::<Message>(&malformed).is_err());
    }

    #[test]
    fn legacy_start_defaults_to_reassembly_and_ambiguous_envelopes_fail() {
        #[derive(Serialize)]
        struct LegacyStart {
            r#type: &'static str,
            index: usize,
            result: ResultSet,
        }
        let bytes = rmp_serde::to_vec_named(&LegacyStart {
            r#type: "start",
            index: 0,
            result: ResultSet::default(),
        })
        .unwrap();
        assert!(matches!(
            rmp_serde::from_slice::<ExecuteChunk>(&bytes).unwrap(),
            ExecuteChunk::Start {
                streaming: false,
                ..
            }
        ));
        #[derive(Serialize)]
        struct Ambiguous {
            id: u64,
            result: Reply,
            chunk: ExecuteChunk,
        }
        let bytes = rmp_serde::to_vec_named(&Ambiguous {
            id: 1,
            result: Reply::Unit,
            chunk: ExecuteChunk::Rows {
                index: 0,
                rows: vec![],
            },
        })
        .unwrap();
        assert!(rmp_serde::from_slice::<Message>(&bytes).is_err());
    }

    #[test]
    fn backup_restore_request_preserves_binary_through_request_envelope(){
        let backup=BackupData{format:"sqlite-online-v1".into(),data:vec![255;BACKUP_MAX_BYTES],counts:vec![("table".into(),1)]};
        let message=Message::Request{id:42,request:Request::Restore{session:1,backup:backup.clone(),overwrite:false}};
        let bytes=encode(&message).unwrap();assert!(bytes.len()<MAX_FRAME);
        let Message::Request{request:Request::Restore{backup:decoded,..},..}=rmp_serde::from_slice::<Message>(&bytes).unwrap() else{panic!("Restore envelope changed");};
        assert_eq!(decoded,backup);
    }

    #[tokio::test]
    async fn fragmented_and_consecutive_frames() {
        let message = Message::Request {
            id: 7,
            request: Request::Handshake { protocol: PROTOCOL },
        };
        let bytes = encode(&message).unwrap();
        let (mut tx, mut rx) = tokio::io::duplex(8);
        let writer = tokio::spawn(async move {
            for _ in 0..2 {
                for byte in (bytes.len() as u32)
                    .to_be_bytes()
                    .into_iter()
                    .chain(bytes.iter().copied())
                {
                    tx.write_all(&[byte]).await.unwrap();
                }
            }
        });
        for _ in 0..2 {
            assert!(matches!(
                read_frame(&mut rx).await.unwrap(),
                Some(Message::Request {
                    id: 7,
                    request: Request::Handshake { protocol: 1 }
                })
            ));
        }
        writer.await.unwrap();
        assert!(read_frame(&mut rx).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn invalid_and_partial_frames_fail() {
        for bytes in [
            vec![0, 0],
            vec![0, 0, 0, 0],
            ((MAX_FRAME + 1) as u32).to_be_bytes().to_vec(),
            vec![0, 0, 0, 2, 0xc1],
        ] {
            assert!(read_frame(&mut bytes.as_slice()).await.is_err());
        }
    }

    #[tokio::test]
    async fn typed_large_result_roundtrip_and_order_validation() {
        let mut results = Vec::new();
        let start = ExecuteChunk::Start {
            index: 0,
            streaming: false,
            result: Box::new(ResultSet {
                table: Some(Table {
                    columns: vec![],
                    rows: vec![],
                }),
                ..Default::default()
            }),
        };
        start.append(&mut results).unwrap();
        for n in 0..50 {
            let chunk = Message::Chunk {
                id: 9,
                chunk: ExecuteChunk::Rows {
                    index: 0,
                    rows: (0..CHUNK_ITEMS)
                        .map(|_| {
                            vec![
                                Value::Int("9223372036854775807".into()),
                                Value::Null,
                                Value::Text(format!("한글 {n}")),
                            ]
                        })
                        .collect(),
                },
            };
            let bytes = encode(&chunk).unwrap();
            let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
            frame.extend(bytes);
            let Some(Message::Chunk { chunk, .. }) =
                read_frame(&mut frame.as_slice()).await.unwrap()
            else {
                panic!("expected chunk")
            };
            chunk.append(&mut results).unwrap();
        }
        assert_eq!(results[0].table.as_ref().unwrap().rows.len(), 100000);
        assert!(ExecuteChunk::Rows {
            index: 1,
            rows: vec![]
        }
        .append(&mut results)
        .is_err());
        assert!(ExecuteChunk::Start {
            index: 0,
            streaming: false,
            result: Box::default()
        }
        .append(&mut results)
        .is_err());
    }

    #[tokio::test]
    async fn cancellation_frame_roundtrip() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &Message::Cancel { cancel: 13 })
            .await
            .unwrap();
        assert!(matches!(
            read_frame(&mut bytes.as_slice()).await.unwrap(),
            Some(Message::Cancel { cancel: 13 })
        ));
    }
}
