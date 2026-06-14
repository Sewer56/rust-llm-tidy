#!/usr/bin/env bash
# Hook: format staged Rust files with rustfmt.
#
# Runs after rust-llm-tidy so imports get sorted/grouped there. This
# hook handles general formatting (spacing, wrapping, import merging).
#
# Cross-platform: Linux, macOS (bash 3.2), Windows (Git for Windows bash).
# macOS note: avoids `mapfile` and `set -u` empty-array expansion.
set -eo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

# Collect staged .rs files (added/copied/modified/renamed); skip deletions.
# Skip hand-curated test fixtures: they contain intentional fragments (e.g.
# `mod foo;` with no matching file) that rustfmt cannot resolve.
files=()
while IFS= read -r f; do
  [ -f "$f" ] || continue
  case "$f" in
    */tests/fixtures/*) continue ;;
    */benches/fixtures/*) continue ;;
  esac
  files+=("$f")
done < <(git diff --cached --name-only --diff-filter=ACMR -- '*.rs')

# Nothing to do if no staged Rust files.
if [ "${#files[@]}" -eq 0 ]; then
  exit 0
fi

# rustfmt must be on PATH.
if ! command -v rustfmt >/dev/null 2>&1; then
  echo "rustfmt: not found on PATH, skipping formatting" >&2
  exit 0
fi

echo "rustfmt: formatting ${#files[@]} staged file(s)"

if ! rustfmt --edition 2024 -- "${files[@]}"; then
  echo "rustfmt: formatting failed" >&2
  exit 1
fi

# Re-stage files rustfmt may have rewritten.
git add -- "${files[@]}"
exit 0
