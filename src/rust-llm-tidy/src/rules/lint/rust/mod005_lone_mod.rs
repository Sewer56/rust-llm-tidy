//! Suggest flattening a directory containing only a regular `mod.rs` file.

use crate::reporting::{Diagnostic, Severity};
use crate::rules::registry::CODE_MOD005;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;

const MODULE_ENTRY_FILE_NAME: &str = "mod.rs";

/// Check the actual directory entries, including hidden and ignored files.
///
/// # Errors
/// Returns an error when the containing directory or its entries cannot be read.
pub(crate) fn check(path: &Path) -> io::Result<Option<Diagnostic>> {
    if path.file_name() != Some(OsStr::new(MODULE_ENTRY_FILE_NAME)) {
        return Ok(None);
    }

    // A bare `mod.rs` refers to the current directory, not an empty path.
    let current_dir;
    let directory = match path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        Some(parent) => parent,
        None => {
            current_dir = std::env::current_dir()?;
            &current_dir
        }
    };
    let mut entries = fs::read_dir(directory)?;
    let Some(entry) = entries.next().transpose()? else {
        return Ok(None);
    };
    if entry.file_name() != OsStr::new(MODULE_ENTRY_FILE_NAME)
        || entries.next().transpose()?.is_some()
        || !entry.file_type()?.is_file()
    {
        return Ok(None);
    }

    // Resolve dot components only when they leave no usable directory name.
    let resolved;
    let name = match directory.file_name() {
        Some(name) => name,
        None => {
            resolved = fs::canonicalize(directory)?;
            let Some(name) = resolved.file_name() else {
                return Ok(None);
            };
            name
        }
    };
    let name = name.to_string_lossy();

    Ok(Some(Diagnostic {
        severity: Severity::Hint,
        code: CODE_MOD005,
        title: None,
        message: format!(
            "directory `{name}/` contains only `mod.rs`.\n\n\
             Why: This directory adds a navigation step without grouping other files.\n\n\
             Suggestions:\n\
             - Consider moving `{name}/mod.rs` to `{name}.rs` and removing the empty directory.\n\
             - Check relative `include!`, `include_str!`, `include_bytes!`, and `#[path]` paths,\n  \
               and explicit references to the old location. Preserve behavior; do not overwrite an existing file."
        ),
        line: 1,
        item_kind: "file".into(),
        item_name: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::check;
    use rstest::rstest;
    use std::fs;

    // Core behavior.

    /// Every actual entry counts, even if source discovery would ignore it.
    #[rstest]
    #[case::lone_mod(None, true)]
    #[case::rust_sibling(Some("child.rs"), false)]
    #[case::asset(Some("data.bin"), false)]
    #[case::hidden_file(Some(".keep"), false)]
    #[case::subdirectory(Some("children/"), false)]
    #[case::hidden_directory(Some(".hidden/"), false)]
    fn check_should_report_only_when_mod_is_the_only_entry(
        #[case] extra_entry: Option<&str>,
        #[case] expected: bool,
    ) {
        // Arrange.
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("banana");
        fs::create_dir(&directory).unwrap();
        let path = directory.join("mod.rs");
        fs::write(&path, "//! Banana.\n").unwrap();
        if let Some(extra) = extra_entry {
            let extra_path = directory.join(extra);
            if extra.ends_with('/') {
                fs::create_dir(extra_path).unwrap();
            } else {
                fs::write(extra_path, "").unwrap();
            }
        }

        // Act.
        let diagnostic = check(&path).unwrap();

        // Assert.
        assert_eq!(diagnostic.is_some(), expected);
    }

    // Edge cases.

    /// Other filenames and a directory named `mod.rs` are not candidates.
    #[rstest]
    #[case::ordinary_file("banana.rs", false)]
    #[case::uppercase_file("MOD.rs", false)]
    #[case::directory("mod.rs", true)]
    fn check_should_skip_when_entry_is_not_a_regular_mod_file(
        #[case] filename: &str,
        #[case] is_directory: bool,
    ) {
        // Arrange.
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(filename);
        if is_directory {
            fs::create_dir(&path).unwrap();
        } else {
            fs::write(&path, "").unwrap();
        }

        // Act.
        let diagnostic = check(&path).unwrap();

        // Assert.
        assert!(diagnostic.is_none());
    }

    /// Unreadable layout is an error, not evidence that the directory is empty.
    #[test]
    fn check_should_return_error_when_directory_is_missing() {
        // Arrange.
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("missing/mod.rs");

        // Act.
        let result = check(&path);

        // Assert.
        assert!(result.is_err());
    }

    /// A symlink is not a regular directory entry and may refer outside the module.
    #[cfg(unix)]
    #[test]
    fn check_should_skip_when_mod_is_a_symlink() {
        // Arrange.
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.rs");
        fs::write(&source, "").unwrap();
        let directory = root.path().join("banana");
        fs::create_dir(&directory).unwrap();
        let path = directory.join("mod.rs");
        std::os::unix::fs::symlink(&source, &path).unwrap();

        // Act.
        let diagnostic = check(&path).unwrap();

        // Assert.
        assert!(diagnostic.is_none());
    }
}
