//! One external post-processing command under the `post_process` key.

use serde::Deserialize;

/// One external post-processing step. The processed file path is appended as
/// the last argument by the library's file pipeline when permission is granted.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)] // Reject hallucinated config keys at parse time.
pub struct PostProcessStep {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Empty = run on every file regardless of extension.
    #[serde(default)]
    pub extensions: Vec<String>,
}
