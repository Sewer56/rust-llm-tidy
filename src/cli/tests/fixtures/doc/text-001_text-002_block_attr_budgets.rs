/** Loads the configured data from disk, parses it, validates every
 * field against the schema, resolves relative paths against the
 * config root, retries transient failures with bounded backoff, and
 * logs a settling summary that runs deliberately past the budget.
 */
fn load() {}

#[doc = " aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
fn long_attr() {}

/** prose
 * bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
 */
fn long_block() {}

/// A short line comment keeps measuring exactly as before.
fn noted() {}

/* Plain block comments stay unmeasured: their joined prose deliberately runs far past
 * both paragraph budgets when mis-measured, and one line here runs past the
 * eighty-character line budget with room to spare, so any regression fails right here.
 */
fn quiet_blocks() {}

#[doc = " Saves the parsed value under the configured key in the backing store, retries"]
#[doc = " each transient failure with bounded exponential backoff, and logs a"]
#[doc = " one-line summary note when the save settles; the joined attribute"]
#[doc = " prose deliberately runs past the two-hundred-forty budget too."]
fn save() {}
