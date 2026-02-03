//! Common types shared between AI adapters (Claude, Codex, etc.)

use serde::Deserialize;

/// Raw reviewer output structure shared by all adapters.
#[derive(Debug, Deserialize)]
pub(crate) struct RawReviewerOutput {
    pub action: String,
    pub summary: String,
    pub comments: Vec<RawReviewComment>,
    pub blocking_issues: Vec<String>,
}

/// Raw review comment structure.
#[derive(Debug, Deserialize)]
pub(crate) struct RawReviewComment {
    pub path: String,
    pub line: u32,
    pub body: String,
    pub severity: String,
}

/// Raw reviewee output structure shared by all adapters.
#[derive(Debug, Deserialize)]
pub(crate) struct RawRevieweeOutput {
    pub status: String,
    pub summary: String,
    pub files_modified: Vec<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub permission_request: Option<RawPermissionRequest>,
    #[serde(default)]
    pub error_details: Option<String>,
}

/// Raw permission request structure.
#[derive(Debug, Deserialize)]
pub(crate) struct RawPermissionRequest {
    pub action: String,
    pub reason: String,
}
