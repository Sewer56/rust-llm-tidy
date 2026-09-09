//! TEXT007 narration-marker and passive-voice reminders.
//!
//! One test audits an ordinary markdown file in both
//! stderr and JSON modes. The other walks the release-note suppression
//! matrix with per-run configuration.

use crate::common::binary;
use crate::{run_command, temp_dir, temp_named_file, text007_marker_and_passive_md};
use std::fs;
use std::process::Command;

/// Config controls narration suppression without hiding passive reminders.
#[test]
fn narration_should_follow_suppression_setting_when_checking_note_paths() {
    for (yaml, suppress) in [
        (None, true),
        (Some("{}\n"), true),
        (
            Some("passive_narration:\n  suppress_in_release_notes: true\n"),
            true,
        ),
        (
            Some("passive_narration:\n  suppress_in_release_notes: false\n"),
            false,
        ),
    ] {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".git"), "").unwrap();
        if let Some(yaml) = yaml {
            fs::write(dir.join(".rust-llm-tidy.yml"), yaml).unwrap();
        }

        for (rel, is_note) in [
            ("CHANGELOG.md", true),
            ("MIGRATION.md", true),
            ("releases/notes.md", true),
            // Case variants match the same release-note paths.
            ("ChangeLog.md", true),
            ("migrationNotes.txt", true),
            ("RELEASES/notes.md", true),
            ("notes.md", false),
        ] {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text007_marker_and_passive_md()).unwrap();

            let output = Command::new(binary())
                .current_dir(&dir)
                .args(["--include", "TEXT007", "--all-lines"])
                .arg(&path)
                .output()
                .unwrap();

            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{rel}, {yaml:?}: {stderr}");
            assert_eq!(
                stderr.contains(":1: reminder[TEXT007]"),
                !(suppress && is_note),
                "narration in {rel}, {yaml:?}: {stderr}"
            );
            assert!(
                stderr.contains(":2: reminder[TEXT007]"),
                "passive voice in {rel}, {yaml:?}: {stderr}"
            );
        }

        fs::remove_dir_all(dir).unwrap();
    }
}

/// An ordinarily named markdown file yields both TEXT007 classes, one
/// per offending line, when auditing all lines.
#[test]
fn text007_should_render_reminders_when_checking_an_ordinary_file() {
    let path = temp_named_file("notes.md", &text007_marker_and_passive_md());
    let output = run_command(&["--include", "TEXT007"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "TEXT007 reminders must not fail the run: {stderr}"
    );
    assert!(
        stderr.contains(":1: reminder[TEXT007]") && stderr.contains(":2: reminder[TEXT007]"),
        "both finding classes must emit reminders:\n{stderr}"
    );

    let output = run_command(&["--include", "TEXT007", "--json"], &path);
    let records: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert!(output.status.success());
    let records = records.as_array().unwrap();
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|record| record["severity"] == "reminder")
    );
}
