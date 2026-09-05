"""
Loads the configured data from disk, parses every record against the
schema, resolves relative paths, retries transient failures with a
bounded backoff policy, and logs a one-line summary once the load
settles into a steady state that operators can rely on every time.
"""

payload = """
not a comment: this triple-quoted string is plain string content,
and these payload lines join past the two hundred forty paragraph
budget, so a docstring gate that wrongly measured the assignment
would fire the paragraph check while correct runs stay quiet here
"""

def load(payload):
    """
    Loads the payload with a doctest example that stays exempt.

    >>> value = load(payload)
    >>> process(value_that_runs_far_past_eighty_chars_and_would_warn_when_measured)
    ... continued call chaining across doctest example source lines
    expected output that is also far past eighty characters and stays exempt
    """
    return payload

def wide():
    """xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"""
    return 1
