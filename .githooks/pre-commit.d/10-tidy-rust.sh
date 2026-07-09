#!/usr/bin/env bash
# Hook: run rust-llm-tidy `all` on staged Rust files.
#
# Cross-platform: Linux, macOS (bash 3.2), Windows (Git for Windows bash).
# macOS note: avoids `mapfile` and `set -u` empty-array expansion.
set -eo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

# Collect staged .rs files (added/copied/modified/renamed); skip deletions.
# Skip deletions only. Hand-curated test fixtures are excluded by the
# repo-root `.rust-llm-tidy.yml` (auto-discovered when this hook runs
# `rust-llm-tidy all` from repo root), so no shell filtering is needed here.
files=()
while IFS= read -r f; do
  [ -f "$f" ] || continue
  files+=("$f")
done < <(git diff --cached --name-only --diff-filter=ACMR -- '*.rs')

# Nothing to do if no staged Rust files.
if [ "${#files[@]}" -eq 0 ]; then
  exit 0
fi

# Prefer an installed `rust-llm-tidy` binary; fall back to `cargo run`
# against this workspace so the hook works with no global install.
if command -v rust-llm-tidy >/dev/null 2>&1; then
  tidy=(rust-llm-tidy)
else
  tidy=(cargo run --quiet --manifest-path src/Cargo.toml -p rust-llm-tidy-cli --)
fi

echo "rust-llm-tidy: running all on ${#files[@]} staged file(s)"

if ! "${tidy[@]}" all "${files[@]}"; then
  echo "rust-llm-tidy: all failed" >&2
  exit 1
fi

# Re-stage files rust-llm-tidy may have rewritten.
git add -- "${files[@]}"
exit 0
