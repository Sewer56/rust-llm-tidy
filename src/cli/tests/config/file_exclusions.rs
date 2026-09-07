//! License-document exclusion: `exclude_license_documents` guards
//! LICENSE-named files; turning it off restores processing via filters.

use super::common::binary;
use super::{git, temp_dir};
use std::fs;
use std::process::Command;

#[rstest::rstest]
#[case::no_configuration(None, true)]
#[case::omitted_license_exclusion(Some("{}\n"), true)]
#[case::enabled_license_exclusion(Some("exclude_license_documents: true\n"), true)]
#[case::disabled_license_exclusion(Some("exclude_license_documents: false\n"), false)]
fn cli_should_follow_license_exclusion_when_configuration_changes(
    #[case] yaml: Option<&str>,
    #[values("directory", "explicit_files", "git_changed")] input_mode: &str,
    #[case] excluded: bool,
) {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "--quiet"]);
    let license = dir.join("LICENSE-MIT.md");
    let guide = dir.join("licenses.md");
    let source = "Read [guide](https://example.com).\n";
    let cfg = dir.join(".rust-llm-tidy.yml");

    if let Some(yaml) = yaml {
        fs::write(&cfg, yaml).unwrap();
    }

    fs::write(&license, source).unwrap();
    fs::write(&guide, source).unwrap();
    git(&dir, &["add", "--", "LICENSE-MIT.md", "licenses.md"]);
    let paths = match input_mode {
        "directory" => vec![&dir],
        "explicit_files" => vec![&license, &guide],
        "git_changed" => vec![],
        _ => unreachable!(),
    };

    let mut command = Command::new(binary());
    command.current_dir(&dir).args(["--include", "links"]);
    if yaml.is_some() {
        command.arg("--config").arg(&cfg);
    }

    let output = command.args(paths).output().unwrap();

    assert!(
        output.status.success(),
        "{yaml:?}, input_mode={input_mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rendered_guide = fs::read_to_string(&guide).unwrap();
    assert_ne!(rendered_guide, source);
    assert_eq!(
        fs::read_to_string(&license).unwrap(),
        if excluded { source } else { &rendered_guide },
        "{yaml:?}, input_mode={input_mode}"
    );

    fs::remove_dir_all(dir).unwrap();
}

#[rstest::rstest]
#[case::excluded_file("LICENSE-MIT.md", "exclude_files: [LICENSE-MIT.md]\n")]
#[case::unselected_extension("LICENSE-MIT.txt", "extensions: [md]\n")]
fn cli_should_preserve_filtered_license_when_license_exclusion_is_disabled(
    #[case] filename: &str,
    #[case] selection: &str,
) {
    let dir = temp_dir();
    fs::create_dir_all(dir.join(".git")).unwrap();
    let cfg = dir.join(".rust-llm-tidy.yml");
    fs::write(
        &cfg,
        format!("exclude_license_documents: false\n{selection}"),
    )
    .unwrap();

    let license = dir.join(filename);
    let guide = dir.join("guide.md");
    let source = "Read [guide](https://example.com).\n";
    fs::write(&license, source).unwrap();
    fs::write(&guide, source).unwrap();

    let output = Command::new(binary())
        .current_dir(&dir)
        .arg("--config")
        .arg(&cfg)
        .args(["--include", "links"])
        .arg(&dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(&license).unwrap(), source);
    assert_ne!(fs::read_to_string(&guide).unwrap(), source);

    fs::remove_dir_all(dir).unwrap();
}
