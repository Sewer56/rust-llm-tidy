//! TEXT007 opt-in tests: passive narration stays off in default lint runs
//! unless the config enables it or the code is explicitly included.

use super::common::binary;
use super::temp_dir;
use std::fs;
use std::process::Command;

/// TEXT007 stays off in default lint runs unless the config enables it or
/// the code is explicitly included.
#[test]
fn text007_should_follow_opt_in_switch_and_explicit_inclusion() {
    for (yaml, args, expected) in [
        // A default run with no selection also keeps the code off.
        ("{}\n", &[][..], false),
        // Off by default, even with the whole `lints` group selected.
        ("{}\n", &["--include", "lints"][..], false),
        (
            "passive_narration:\n  suppress_in_release_notes: false\n",
            &["--include", "lints"],
            false,
        ),
        // The `enable` switch opts the code into the selected and default
        // pipelines alike.
        (
            "passive_narration:\n  enable: true\n",
            &["--include", "lints"],
            true,
        ),
        ("passive_narration:\n  enable: true\n", &[], true),
        // Naming the code always runs it, `enable` or not.
        (
            "passive_narration:\n  enable: false\n",
            &["--include", "TEXT007"],
            true,
        ),
        // A config whitelist naming the code also opts in; `lints` alone
        // does not.
        ("include:\n  - rules: [TEXT007]\n", &[], true),
        ("include:\n  - rules: [lints]\n", &[], false),
    ] {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join(".rust-llm-tidy.yml");
        fs::write(&cfg, yaml).unwrap();
        let file = dir.join("notes.md");
        fs::write(&file, "Errors are returned by the scanner.\n").unwrap();

        let output = Command::new(binary())
            .arg("--config")
            .arg(&cfg)
            .args(args)
            .arg(&file)
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{yaml}, {args:?}: {stderr}");
        assert_eq!(
            stderr.contains("hint[TEXT007]"),
            expected,
            "{yaml}, {args:?}: {stderr}"
        );

        fs::remove_dir_all(dir).unwrap();
    }
}
