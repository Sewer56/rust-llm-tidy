/// Loads the configured data, parses every record, resolves relative paths,
/// retries transient failures, and logs one summary note once the load
/// settles into place today.
fn at_limit() {}

/// Saves the parsed value under the configured key in the backing store,
/// retries each transient failure with bounded exponential backoff, and
/// logs one final summary note when the save settles for good.
fn over_limit() {}

/// One short opener sentence.
/// The joining scanner counts words across wrapped doc lines, so a sentence
/// that starts on one line and keeps running on the next two lines still
/// reports once at the line where its first measured word starts.
fn wrapped() {}
