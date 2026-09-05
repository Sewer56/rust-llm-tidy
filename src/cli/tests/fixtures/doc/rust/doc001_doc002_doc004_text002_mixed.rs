// One file mixing item-rule and text-tier codes for the CLI parity
// test; it mirrors the inline source of the backend order unit test.
pub fn load(path: &str, fmt: &str) -> Result<(), String> {
    let _ = (path, fmt);
    Ok(())
}

/// A documented function whose doc line runs past the eighty character budget limit for lines.
pub fn documented() {}

pub fn bare() {}
