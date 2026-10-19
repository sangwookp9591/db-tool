//! A proposal, never an execution instruction or approval credential.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Explain,
    Analyze,
    Refactor,
    Import,
    SchemaDiff,
    Migrate,
    Backup,
    Restore,
    Grant,
    ConnectTest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationRisk {
    Read,
    Write,
    Destructive,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub kind: OperationKind,
    pub connection: String,
    pub params: serde_json::Value,
    pub plan: Option<serde_json::Value>,
    pub risk: OperationRisk,
    pub needs_approval: bool,
}
