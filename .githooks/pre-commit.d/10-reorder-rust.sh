#!/usr/bin/env bash
# Hook: reorder staged Rust files with rust-auto-reorder.
#
# Cross-platform: Linux, macOS (bash 3.2), Windows (Git for Windows bash).
# macOS note: avoids `mapfile` and `set -u` empty-array expansion.
set -eo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

# Collect staged .rs files (added/copied/modified/renamed); skip deletions.
# Skip hand-curated test fixtures: they are intentional before/after pairs,
# not compilable units, so reordering would corrupt them.
files=()
while IFS= read -r f; do
  [ -f "$f" ] || continue
  case "$f" in
    */tests/fixtures/*) continue ;;
  esac
  files+=("$f")
done < <(git diff --cached --name-only --diff-filter=ACMR -- '*.rs')

# Nothing to do if no staged Rust files.
if [ "${#files[@]}" -eq 0 ]; then
  exit 0
fi

# Prefer an installed `rust-auto-reorder` binary; fall back to `cargo run`
# against this workspace so the hook works with no global install.
if command -v rust-auto-reorder >/dev/null 2>&1; then
  reorder=(rust-auto-reorder)
else
  reorder=(cargo run --quiet --manifest-path src/Cargo.toml -p rust-auto-reorder-cli --)
fi

echo "rust-auto-reorder: reordering ${#files[@]} staged file(s)"

if ! "${reorder[@]}" reorder "${files[@]}"; then
  echo "rust-auto-reorder: reorder failed" >&2
  exit 1
fi

# Re-stage files the reorder step may have rewritten.
git add -- "${files[@]}"
exit 0
