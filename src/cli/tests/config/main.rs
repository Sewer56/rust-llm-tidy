//! Integration tests for the `--validate` mode and config-driven
//! exclusions of `rust-llm-tidy`.
//!
//! Mirrors the helper pattern from `fix.rs`/`doc_check/mod.rs` (`run_command`,
//! `manifest_dir`; `binary` lives in the shared `common` module).
//!
//! Config fixtures use `--config <path>`; discovery tests isolate their working
//! directory so the repo-root sample config cannot affect their results.
//!
//! Suite map:
//! - `cli_selection`: `--include`/`--exclude` flags and default-mode effects.
//! - `config_selection`: config `include:`/`exclude:` mode selection.
//! - `discovery`: config auto-discovery and pattern anchoring.
//! - `extensions`: `extensions:`/`extra_extensions:` selection keys.
//! - `file_exclusions`: license-document file exclusion.
//! - `passive_narration`: TEXT007 enablement and reporting scope.
//! - `post_process`: `post_process:` gating and exit propagation.
//! - `validation`: `--validate` acceptance and failure modes.

use core::sync::atomic::{AtomicU64, Ordering};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

mod cli_selection;
mod config_selection;
mod discovery;
mod extensions;
mod file_exclusions;
mod passive_narration;
mod post_process;
mod validation;
// The folder root sits inside `tests/config/`, so the helpers shared by
// every test binary resolve at their sibling path, not under this folder.
#[path = "../common/mod.rs"]
mod common;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

// -- Helpers (mirrors fix.rs) -----------------------------------

/// Run Git inside a temporary fixture without changing the process directory.
fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .expect("failed to run git");

    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Create a numbered temporary directory.
fn temp_dir() -> PathBuf {
    let seq = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = process::id();
    std::env::temp_dir().join(format!("rust-llm-tidy-cfg-dir-{}-{}", pid, seq))
}
