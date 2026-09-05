const template = `// This template literal line looks exactly like a genuinely very long comment line here
// and it runs far past both text budgets when its lines join together with all of
// their neighbors inside the literal, so mis-measuring it would fire the findings`;

const url = "//not-a-comment-either-but-a-string-constant-whose-tail-runs-past-eighty-chars-here";

// A short comment.
function quiet(template, url) {
  return template + url;
}
