# Action v2 draft

Move reusable Git selection and recovery into rust-llm-tidy, leaving the action
responsible for installation and GitHub integration.

## Status and scope

Draft for discussion, not an implemented API or release announcement.
Investigated parent commit `5ddbaa105283387ccd2fa077cde75c04e67e41b2` and action
commit `eb29f9a37a30e1c283ea6dbcf0f700c8e4f0399b`.

Deliver the CLI changes first, then an action v2 requiring the release containing
them. Do not remove recovery from the published v1 action while it supports older
binaries. Do not change tags as part of this plan.

## What the current code establishes

Paths below are relative to the parent repository.

| Evidence | Consequence for v2 |
| --- | --- |
| `src/cli/src/args.rs`: `diff_base`, `all_lines`, `extension` | Expose these features through named action inputs. |
| `src/rust-llm-tidy/src/pipeline/mod.rs`: `run` | Baseline plus omitted paths currently means recursive discovery, not changed-file selection. Keep the distinction explicit. |
| `src/rust-llm-tidy/src/pipeline/lint_context.rs`: `capture` | Explicit baselines are validated even without scoped files. Truly lazy recovery needs a deliberate contract change. |
| `src/rust-llm-tidy/src/input/changed_lines/collection.rs`: `collect` | Public library collection explicitly disallows remote fetching. Preserve the default boundary. |
| `submodules/action/action.yml`: argument and run steps | History recovery runs both before PR file selection and before the CLI. Move both uses behind one resolver. |
| `submodules/action/action.yml`: empty `changed` array | With a PR baseline exported, no changed files can become recursive CLI discovery. An empty selection must remain empty. |
| `submodules/action/action.yml`: validation branch | `--validate` omits the requested config path and config-discovery flag. Forward config options before branching. |
| `submodules/action/action.yml`: successful apply push | `exit 0` discards the CLI's failure status. Publishing fixes must not turn remaining errors into success. |
| `submodules/action/scripts/finding_compare.py`: `FINDING_SEVERITIES` | `reminder` findings never enter sticky comparisons. |
| `submodules/action/scripts/json_table.py`: `main` | Rendering groups omit reminders, which the CLI now emits. |
| `src/rust-llm-tidy/src/reporting/run_report.rs`: `ensure_success` | A would-be edit alone does not fail preview. A real CLI check mode needs its own exit contract. |
| `submodules/action/scripts/head_view.py`: `_scan_worktree` | Recollection rejects submodules and accepts any JSON list despite a failing process. Report completeness needs a stronger signal. |

## Ownership

### Move into the CLI and shared Git implementation

- Baseline resolution, missing-reference recovery, and shallow-history recovery.
- Changed-file discovery against an explicit baseline, with correct path handling.
- A check-mode exit contract that works locally and in CI.
- A machine-readable run outcome that distinguishes findings from incomplete work.

Keep Git mechanics in focused modules under the library's `input` boundary.
The CLI selects network permission; the library must not infer permission from
GitHub environment variables or silently expand existing local-only APIs.
Reuse the resolver for discovery and changed-line snapshots within one run.

### Keep in the action

- Binary acquisition, source builds, toolchain setup, and caches.
- Selecting the default PR baseline from the GitHub event.
- Commit and push operations, token permissions, and fork restrictions.
- GitHub comments, sticky state, revision links, pagination, and stale-run checks.
- GitHub job outputs and summaries.

Do not make the CLI a GitHub client, release downloader, or automatic commit tool.
The action should translate inputs to an argument array, not interpret lint
configuration or rediscover which rules need changed lines.

### Defer until independently useful

Keep Markdown rendering and finding comparison in the action for the initial v2.
They encode publication policy, including retaining original revision links.
Moving them would add a CLI reporting interface without eliminating GitHub state.

Keep committed-revision recollection in the action initially. A general CLI
revision-scan feature would need its own rules for config paths, submodules,
external tools, and temporary checkouts. A non-mutating check mode can remove its
normal check-mode use without introducing that interface.

## Proposed CLI behavior

Names below are proposed, not existing options.

### File selection and line reporting stay independent

Add `--changed-files`. Preserve existing defaults when it is absent.

| Invocation | Selected files |
| --- | --- |
| No paths, no baseline, no new flag | Existing staged plus unstaged tracked-file selection. |
| `--diff-base REF` without paths | Existing recursive discovery under cwd. |
| `--changed-files --diff-base REF` | Existing files changed between the merge-base and HEAD. |
| `--changed-files` without a baseline | Existing local tracked-change selection. |
| Explicit paths without `--changed-files` | Existing explicit-path expansion. |

For the first version, reject explicit paths combined with `--changed-files`
rather than invent another intersection rule. The action preserves its existing
explicit-path precedence by not passing the flag in that case.

Baseline-based selection matches the action's committed PR comparison. Local
unstaged or staged edits to other files are not silently added to that selection.
Changed-line reporting still compares the selected files' current bytes against
the merge-base. Document this difference for local use.

Resolve names from the repository root and return paths usable from project
subdirectories. Retain extension and license-document filters. Use NUL-delimited
Git output, preserve supported non-UTF-8 paths, exclude deletions, and do not
recurse through submodule commit IDs as though they were ordinary source files.
An empty diff is an explicit empty selection, never omitted-input discovery.

`--all-lines` changes reporting eligibility, not file selection. It must not
disable a baseline needed by `--changed-files`.

### Recover missing history when it is needed

CLI default: automatic recovery from the repository's configured remote when
baseline-based discovery or an enabled changed-line rule needs unavailable
history. Provide `--no-fetch` for local-only execution.

Library default: local-only. Add a narrowly named fetch permission to the new
execution path; preserve existing standalone collection APIs as local-only.
Do not enable fetching through configuration stored in an untrusted repository.

Recovery sequence:

1. Resolve HEAD, the requested baseline, and their merge-base locally.
2. If the baseline is missing and maps unambiguously to a remote ref or commit,
   fetch that endpoint without tags or submodules.
3. If shallow boundaries still prevent resolution, deepen the relevant history
   in bounded steps, retrying local resolution after each fetch.
4. Fail with the operation, cause, and a manual recovery command if the history
   budget is exhausted or the remote cannot supply the required history.

Use the remote named by a remote-tracking ref. For a raw commit ID, use `origin`;
fail clearly if absent. Do not guess among multiple remotes, treat arbitrary
revision expressions as fetch refspecs, or substitute the current remote tip for
an unavailable requested commit. Locally resolvable expressions remain valid.

Do not fetch for implicit HEAD-versus-working-tree comparisons, validation,
unrelated complete histories, or a run with no baseline consumer. Do not retry
authentication, certificate, permission, or repository-integrity failures as
though they were shallow-history failures.

Fetches may update object storage and remote metadata, but must not checkout,
reset, stash, commit, rewrite the index, or modify source files. Use existing Git
credentials without exposing them; disable interactive prompts for automatic
recovery and send progress to stderr so JSON stdout remains parseable.
Never recursively fetch PR-controlled submodule URLs.

Set finite deepening, elapsed-time, and captured-output budgets before shipping.
Initial proposed depth targets are 64, 256, 1024, 4096, and 16384 commits, with a
120-second total recovery deadline. These are design defaults to validate, not
measured guarantees. Commit depth does not bound transferred bytes; do not claim
it does. Prefer an actionable manual `git fetch --unshallow` instruction over an
unbounded automatic full-history fallback.

### Make unused baselines lazy

Today an explicit baseline is validated even if every selected rule reports all
lines, all files are excluded, or no files are selected. Keeping that behavior
would prevent the desired no-work/no-fetch optimization.

Proposal: in the new CLI execution path, resolve a baseline only for discovery
or selected changed-line consumers. Validate argument syntax up front, but do
not fail solely because an unused reference is absent locally. Explicit
baseline-validation APIs remain strict and local-only.

Treat this as an intentional CLI compatibility change. Keep tests proving that
a baseline failure never broadens a needed changed-line scope to all lines.
Account for different repositories among explicit inputs: a PR base SHA is not
a valid default baseline for unrelated submodules. Do not fetch it into each
nested repository in an attempt to make it valid.

### Give check mode a CLI contract

Add `--check`: do not write source files, fail for required transformations or
error-severity findings, and preserve failures from processing. Keep `--dry-run`
as preview with its existing status contract; reject using both flags together.

Do not claim preview equals apply. Current file execution previews individual
operations without persisting their intermediate results, and preview skips
post-processing. A first CLI check can honestly check built-in transformations
and original-source findings without running configured external formatters.
Changing action check mode to this behavior is a v2 migration decision.

If exact apply-plus-post-process parity is required, design and test isolated
execution separately. That is substantially larger than adding an exit flag and
must not be hidden inside the history-recovery work.

## Proposed action v2 interface

Retain installation inputs and the existing project path, paths, selection,
config, validation, and mode inputs. Add the missing named controls:

| Input | Mapping |
| --- | --- |
| `diff-base` | `--diff-base REF`; nonempty input wins over the baseline environment variable and event default. |
| `all-lines` | `--all-lines`; default false. |
| `extensions` | One extension per line, mapped to repeated `--extension`. |
| `fetch-history` | Default true; false maps to `--no-fetch`. |
| `changed-files` | Pass `--changed-files` only when explicit paths are absent. |
| `mode: check` | Proposed `--check`; no working-tree mutation. |
| `mode: apply` | Normal CLI apply, followed by action-owned commit/push when permitted. |

Keep output format action-owned. Do not add an unrestricted shell command or
shell-split `extra-args` input. Pass values through environment variables into
argument arrays and put `--` before positional paths.

Baseline precedence: named input, existing `RUST_LLM_TIDY_DIFF_BASE`, PR event
default, then no explicit baseline outside PRs. Validate boolean and mode inputs
instead of interpreting unknown values as apply mode. Forward config selection
for validation too. Reject `mode: check` plus `dry-run: true` with a clear message.

Check mode should be the v2 default unless compatibility is preferred over
avoiding automatic pushes. This default change needs approval.

## Results and publishing

Expose a versioned machine report alongside the existing JSON record array.
Keep the old output format compatible for other consumers. The new report should
carry findings, transformation records, selected-file outcomes, warnings,
processing failures, completion status, and the resolved comparison identity.
Do not interpret valid `[]` output as proof that processing completed.

A changed-file manifest must describe actual writes, not just transformation
messages. Configured post-process commands can modify additional paths; do not
claim an exact write manifest until that behavior is accounted for.

For initial v2 apply runs, require a clean index and working tree before running
the CLI. Never stage unrelated pre-existing changes. Detect action-owned changes
with NUL-delimited Git output and restrict commits to the authorized repository.
Do not commit partial output after execution failures. Remaining lint errors may
coexist with valid fixes, but a successful push must preserve the failing lint
outcome rather than turn the job green.

Render and compare `reminder` alongside `error`, `warning`, and `hint`, preserving
their non-failing semantics. Update sticky counts, snapshots, truncation, and
rendering together. Incomplete scans must not clear previous findings.

Only publish committed revision links for findings known to describe that
revision. Apply reports currently precede external post-processing, so pushing
does not prove every reported location describes final committed bytes. Re-scan
the committed result or explicitly preserve source provenance. If a push fails,
retain safe head recollection or skip publication rather than mislabel local fixes.

## Delivery order

1. Implement shared baseline recovery with explicit library permission, lazy CLI
   consumption, and deterministic local-remote tests.
2. Implement CLI baseline-based changed-file selection, including empty results.
3. Release the CLI containing those features. Add reliable version or capability
   identification before rejecting unsupported binaries across all install modes.
4. Wire action v2 to the CLI, add missing inputs and reminder support, then remove
   `ensure_diff_base.sh` and shell PR-diff selection from v2 only.
5. Deliver CLI check and complete machine-report contracts, then simplify action
   status and publication handling. Keep this separable from history recovery.
6. Publish v2 after cross-platform validation; leave v1 available. Update this
   repository's consumer and submodule pointer only after the release exists.

Pin a supported CLI release by default in v2 rather than silently relying on
whichever release is latest. Reject older PATH binaries and source builds before
mutating the user's checkout. Do not maintain two execution algorithms inside v2.

## Acceptance tests

Extend existing CLI Git-diff and reporting-scope suites, plus action script and
workflow tests. Keep fixtures hermetic with local bare remotes and controlled Git
configuration. New independent Rust cases use named `rstest` cases.

- Compare final selected paths and consumed findings from shallow and complete
  clones in the same test. Cover missing base, present base with missing ancestry,
  detached HEAD, diverged branches, and restricted fetch refspecs.
- Prove no fetch for available history, `--no-fetch`, implicit local changes,
  validation, excluded rules, and all-line-only runs without diff selection.
- Cover unavailable origin, denied fetch, unrelated histories, exhausted budgets,
  and malformed refs. Verify failure never changes HEAD, index, or source bytes.
- Cover empty and deletion-only diffs, renames, spaces, newlines, leading dashes,
  project subdirectories, extensions, license files, and nested repositories.
- Execute the CLI directly and through the action adapter against equivalent
  fixtures and compare selected paths, findings, final bytes, and exit outcomes.
- Check missing inputs, config validation, baseline precedence, every new flag,
  unsupported binary rejection, reminder-only reports, and partial failures.
- Preserve nonzero status after successful apply pushes with remaining errors;
  prove dirty pre-existing work cannot enter an automated commit.
- Verify check mode leaves source unchanged and state its external-tool limits.
  Any future check/apply equivalence claim needs both paths executed and compared.
- Exercise Linux, macOS, and Windows, including Git worktrees and PR-head versus
  merge-ref fixtures. Keep real GitHub token/network validation separate from the
  deterministic suite.

## Decisions to confirm

- Accept default automatic CLI fetch with `--no-fetch` and explicit library opt-in.
- Accept lazy resolution of unused baselines in the new CLI execution path.
- Accept bounded deepening rather than automatic unlimited unshallowing.
- Choose whether v2 check excludes external formatters or requires isolated parity.
- Choose whether action v2 defaults to check instead of apply.

This draft was investigated against local sources, not live GitHub runs. No new
dependency behavior or exact minimum release version has been assumed. Before
implementation, verify fetch/refspec behavior against the supported Git versions
and any new dependency behavior against pinned sources.
