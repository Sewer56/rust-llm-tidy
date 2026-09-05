%% Loads the configured data set from disk, parses every record
%% against the schema, resolves relative paths, retries transient
%% failures, and logs a one-line summary once the load settles
%% into a steady state operators rely on for every run of the tool.

quiet() ->
    <<"not a comment: this binary literal tail runs far past the eighty char budget">>.

%% this trailing note line deliberately runs past the eighty character budget limit for text002 warnings to fire on it
