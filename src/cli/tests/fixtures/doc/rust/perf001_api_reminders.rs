//! Rule: PERF001 - invoking a built-in or configured API emits a
//! hint-severity reminder to consider a capacity-setting alternative.
//!
//! Expected diagnostics:
//! - PERF001 hint on the `out` binding (Vec capacity reminder)
//! - PERF001 hint on the `joined` binding (String capacity reminder)
//! - PERF001 hint on the `counts` binding (HashMap capacity reminder)

fn collect_doubled(items: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for item in items {
        out.push(item * 2);
    }
    out
}

fn join_names(names: &[&str]) -> String {
    let mut joined = String::new();
    for name in names {
        joined.push_str(name);
    }
    joined
}

fn count_words(words: &[&str]) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for word in words {
        *counts.entry(word.to_string()).or_insert(0) += 1;
    }
    counts
}
