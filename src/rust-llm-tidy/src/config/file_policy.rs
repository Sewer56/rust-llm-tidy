//! Runtime per-file policy: the skip flag plus enabled/disabled rule sets.

use std::collections::HashSet;

/// Runtime policy for a single file: whether to skip it entirely, which ops are
/// enabled, and (for blacklist/default mode) which rules are disabled.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FilePolicy {
    /// Matched by an `exclude_files` pattern.
    pub skip: bool,
    /// Ops/rules enabled for this file (whitelist mode) or `None` for the
    /// blacklist/default mode (caller disables via `disabled`).
    pub enabled: Option<HashSet<String>>,
    /// Union of `rules` from all matched `exclude` groups (blacklist/default
    /// mode). Empty in whitelist mode.
    pub disabled: HashSet<String>,
}
