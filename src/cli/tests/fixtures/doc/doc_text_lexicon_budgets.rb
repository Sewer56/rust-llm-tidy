# Loads the configured data from disk, parses every record against
# the schema, resolves relative paths, retries transient failures,
## heading-shaped prose lines still measure as prose rather than pass
# silently through any heading exemption borrowed from markdown files.

list = []
list << :item

text = <<~PAYLOAD
# not a comment: this heredoc payload line runs well past the eighty
# character budget, and the payload lines pool far past the paragraph
# budget when joined, so a scanner that mistook the payload for
# comments would fire both text checks on all of these payload lines
PAYLOAD

def quiet(list, text)
  [list, text]
end
