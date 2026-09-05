#!/bin/sh
# Loads the configured data from disk, parses every record against
# the schema, resolves relative paths, retries transient failures,
# and logs a one-line summary once the load settles into a steady
# state that operators can rely on for every single run of the tool.

payload=$(cat <<EOF
# not a comment: this heredoc payload line runs well past the eighty
# character budget, and the payload lines pool far past the paragraph
# budget when joined together, so a scanner that mistook the payload
# for comments would fire both text checks on all of these lines here
EOF
)

printf '%s\n' "$payload" $#
