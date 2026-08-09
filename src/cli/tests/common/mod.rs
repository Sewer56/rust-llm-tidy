//! Helpers shared by the `rust-llm-tidy` CLI integration tests.
//!
//! Each file under `tests/` is compiled as its own crate, so helpers used by
//! several test binaries live in this submodule (`tests/common/mod.rs`) and
//! are pulled in with `mod common;` per test file.

/// Returns the path to the `rust-llm-tidy` binary for spawning in tests.
///
/// Resolution order:
/// 1. `CARGO_BIN_EXE_rust-llm-tidy`; modern Cargo keeps the hyphen.
/// 2. `CARGO_BIN_EXE_rust_llm_tidy`; older Cargo normalized it.
/// 3. Walk up from the test executable to the `target/<profile>` dir that
///    holds the peer binary.
///
/// Panics when none resolve.
pub fn binary() -> std::path::PathBuf {
    for var in ["CARGO_BIN_EXE_rust-llm-tidy", "CARGO_BIN_EXE_rust_llm_tidy"] {
        if let Some(path) = std::env::var_os(var) {
            return std::path::PathBuf::from(path);
        }
    }

    // Fallback for direct runs: the test binary lives in `<profile>/deps/`
    // (stable) or the build-out dir (newer Cargo); both sit under the
    // `<profile>` dir that holds the peer binary.
    let mut dir = std::env::current_exe()
        .expect("current_exe must resolve")
        .parent()
        .expect("current_exe must have a parent")
        .to_path_buf();
    loop {
        for bin in ["rust-llm-tidy", "rust-llm-tidy.exe"] {
            let candidate = dir.join(bin);
            if candidate.is_file() {
                return candidate;
            }
        }
        if !dir.pop() {
            break;
        }
    }
    panic!("could not locate the rust-llm-tidy binary next to the test executable");
}
