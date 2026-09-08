//! Rule: PERF001 negative - calls that already choose an alternative
//! stay silent, as do API mentions in comments and strings.
//!
//! Expected diagnostics: none.

fn collect_doubled(items: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(item * 2);
    }
    out
}

fn mentioned() -> usize {
    let text = "Vec::new";
    // Vec::new()
    text.len()
}
