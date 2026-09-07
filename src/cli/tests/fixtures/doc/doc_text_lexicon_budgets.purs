{-
Loads the configured data from disk, parses every record against
the schema, resolves relative paths, retries transient failures,
and logs a one-line summary once the load settles into a steady
state that operators can rely on for every single run of the tool.
-}
quiet = 1
