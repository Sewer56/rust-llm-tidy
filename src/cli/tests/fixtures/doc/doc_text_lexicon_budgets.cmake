# Loads the configured data from disk, parses every record against
# the schema, resolves relative paths, retries transient failures,
# and logs a one-line summary once the load settles into a steady
# state that operators can rely on for every single run of the tool.

set(x "starts
# not a comment: this multiline quoted argument payload runs far
# past the eighty character budget and pools past the paragraph
# budget when joined, so a scanner that mistook payload for comment
  ends")
