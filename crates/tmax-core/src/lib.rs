use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod catalog;
pub mod config;
pub mod diagnostics;
pub mod gateway;
pub mod inputs;
pub mod integrations;
pub mod native;
pub mod observations;
pub mod operations;
pub mod routing;
pub mod secrets;
pub mod service;
mod skill;
pub mod storage;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub command: Vec<String>,
    #[serde(default)]
    pub operands: Vec<String>,
    #[serde(default)]
    pub arguments: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub dry_run: bool,
    pub if_revision: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub exit_code: u8,
    pub body: serde_json::Value,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRef {
    pub kind: String,
    pub id: Option<String>,
    pub scope: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    pub code: String,
    pub message: String,
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected: Vec<ResourceRef>,
}
impl Error {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            field: None,
            affected: Vec::new(),
        }
    }
    pub fn field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for Error {}
pub type Result<T, E = Error> = std::result::Result<T, E>;
pub async fn execute(request: Request) -> Response {
    operations::execute(request).await
}
pub fn render_text(response: &Response) -> String {
    serde_json::to_string_pretty(&response.body).unwrap_or_else(|_| "Result unavailable".into())
}
