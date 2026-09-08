//! Read-only Git execution with capped stdout and no lazy object fetches.

use super::MAX_COLLECTION_BYTES;
use anyhow::{Context, Result, bail};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

/// Require successful execution of an already configured Git operation.
pub(super) fn checked(command: &mut Command) -> Result<Vec<u8>> {
    let (status, bytes) = output(command)?;
    if !status.success() {
        bail!("local Git operation failed with {status}");
    }

    Ok(bytes)
}

/// Decode a NUL-delimited Git path without lossy conversion on Unix.
pub(super) fn path(bytes: &[u8]) -> Result<std::path::PathBuf> {
    #[cfg(unix)]
    {
        Ok(std::ffi::OsStr::from_bytes(bytes).into())
    }
    #[cfg(not(unix))]
    {
        Ok(core::str::from_utf8(bytes)
            .context("Git path is not representable as UTF-8 on this platform")?
            .into())
    }
}

/// Capture bounded stdout; kill and reap children before reporting read failures.
///
/// Stderr is discarded rather than retained without a bound. Callers report the
/// operation and repository themselves. Exit status interpretation belongs to
/// the caller because diff and discovery have meaningful nonzero statuses.
pub(super) fn output(command: &mut Command) -> Result<(ExitStatus, Vec<u8>)> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("could not start local Git; check that Git is installed")?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .context("Git stdout pipe was unavailable")?
        .take(MAX_COLLECTION_BYTES as u64 + 1)
        .read_to_end(&mut bytes);

    if read.is_err() || bytes.len() > MAX_COLLECTION_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        read.context("could not read Git output")?;
        bail!("Git output exceeds {MAX_COLLECTION_BYTES} bytes; narrow the input repository");
    }

    let status = child.wait().context("could not wait for local Git")?;
    Ok((status, bytes))
}

/// Build an explicit local Git command, ignoring ambient repository selection.
pub(super) fn command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["--no-pager", "--no-optional-locks", "--no-lazy-fetch"])
        .arg("--literal-pathspecs")
        .args(["-c", "core.fsmonitor=false"])
        .stdin(Stdio::null())
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0");
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_PREFIX",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ] {
        command.env_remove(name);
    }

    command
}
