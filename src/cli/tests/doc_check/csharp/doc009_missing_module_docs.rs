//! DOC009 stays silent for C#: the language has no module header.
//!
//! Every test runs `--include DOC009` on a fixture in
//! `tests/fixtures/doc/csharp/` and asserts on its exit code and stderr
//! diagnostics. The shared runner helper lives in `mod.rs`.

use super::{csharp_fixture_dir, run_command};

/// A `.cs` file passes a DOC009-only run with no diagnostics.
#[test]
fn csharp_doc009_stays_silent_without_a_module_header() {
    let path = csharp_fixture_dir().join("doc009_silent.cs");
    let output = run_command(&["--include", "DOC009"], &path);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "a `.cs` file must pass a DOC009-only run: {stderr}"
    );
    assert!(
        stderr.is_empty(),
        "C# has no module header, so DOC009 must stay silent:\n{stderr}"
    );
}
