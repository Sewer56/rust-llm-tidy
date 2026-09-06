<!-- Benchmark fixture: reloaded-templates-rust agent docs. -->
<!-- medium tier; dirty for fix_fences (nested same-marker fences rewritten). -->
<!-- Source (pinned): https://github.com/Reloaded-Project/reloaded-templates-rust/blob/2f92401e0bc7bc068d81860db81d8b01927f09e6/docs/AGENTS-DOCS.md -->
<!-- Embedded verbatim via include_str! in benches/common.rs. -->

# Agent Guidelines: Documentation Folder

## Documentation Commands
- `mkdocs build --strict` - Check build errors
- NEVER run blocking commands like `mkdocs serve`
- ALWAYS delete `dist/` folder immediately after `mkdocs build --strict`

## Context
These guidelines help you write new pages that match our existing docs while allowing different content depth by topic. Prioritize adoption, clarity, and showing features in action.

## Core Invariants
- **Active voice, concise sentences**: Prefer direct, outcome-focused language.
- **Scannability**: Use headings and bullet lists to organize content.
- **Visuals with captions**: Use `.avif` where visuals add clarity; include a caption.
- **Link hygiene**: Prefer relative links for internal docs; link to official tool docs externally.
- **Admonitions for emphasis**: Use `tip`, `info`, `warning`, `example`, and collapsible `???` blocks.

## Page Types and Recommended Patterns
Use the closest pattern; adapt as needed. These are guidelines, not rules.

- **Tool Integration** (e.g., VSCode, GitHub templates)
  - Short intro (1–3 sentences) with value.
  - Quick Start and/or Integration steps.
  - Key sections by feature (debugging, tasks, coverage, etc.).
  - Screenshots with captions that show features in action.

- **CI/Automation** (e.g., Automated Testing & Publishing)
  - Short intro with promise.
  - “Key Features” list with anchor links to sections.
  - Sections for matrix/testing, coverage, semver checks, publishing.
  - Visuals for CI checks and dashboards.
  - Brief integration steps.

- **Technical Feature** (e.g., Benchmarking & Profiling)
  - Quick Start command(s).
  - Deeper content is OK (longer sections, code blocks, file structure, best practices).
  - Visuals for reports/graphs with captions.
  - Integration steps may be multi-step.

- **Configuration** (e.g., License Options)
  - Options list and/or comparison table.
  - Short context or rationale.
  - Visuals optional.
  - Integration steps for applying in existing projects.

## Visuals
- Prefer `.avif` images with descriptive alt text and captions.
- Include visuals when they clarify outcomes; skip them for configuration pages if not valuable.

Caption format:
```markdown
![Descriptive alt text](../assets/image.avif)
/// caption
Clear explanation of what's shown and why it matters
///
```

## Admonitions
Use for emphasis and scannability:
```markdown
!!! tip "Best Practice"
    Helpful hint for optimal usage.

!!! info "Additional Context"
    Extra useful information.

!!! warning "Important Note"
    Critical information.

!!! example "Real World Use"
    Practical example.

??? info "Advanced Details"
    Collapsible technical details.
```

## Annotations
Inline and list annotations are allowed:
```markdown
Text with annotation (1)
{ .annotate }

1.  This is the annotation content.
```

For lists:
```markdown
<div class="annotate" markdown>

1. item one (1)
2. item two

</div>

1. annotation for item one
```

## Integration vs Quick Start
- Prefer the header `## Integrate with Non-Template Projects`.
- `## Quick Start` is acceptable when it serves the same purpose.
- Keep steps brief and actionable (copy files, minimal commands).
- When helpful, add: `See the [main documentation](../index.md#getting-started) for more details.`

### Example integration block
```markdown
## Integrate with Non-Template Projects
!!! info
    If your project was not built on Reloaded template, here's how you can recreate template parts.

Copy the relevant files from the template to your project, then follow the steps below.

See the [main documentation](../index.md#getting-started) for more details.
```

## Linking Conventions
- **Internal**: Relative links (e.g., `../index.md#getting-started`).
- **Anchors**: In “Key Features”, link to sections like `#code-coverage`.
- **GitHub file links**: Use direct links to template files when instructing users to copy.
- **External**: Link to official documentation (SemVer, Codecov, Criterion, etc.).

## Authoring Snippets
- **Hero image with caption**
```markdown
![PR Checks](../assets/pr-checks.avif)
/// caption
Automated checks running on pull requests
///
```

- **Key Features with anchors**
```markdown
## Key Features
- **[Cross-platform testing](#cross-platform-testing)**: Linux, Windows, macOS
- **[Code coverage](#code-coverage)**: Track test completeness (Codecov)
- **[Semantic version checks](#semantic-version-checks)**: Prevent breaking changes
```

- **Integration boilerplate**
```markdown
## Integrate with Non-Template Projects
!!! info
    If your project was not built on Reloaded template, here's how you can recreate template parts.

[Brief steps or copy instructions]

See the [main documentation](../index.md#getting-started) for more details.
```

- **Technical code blocks**
```markdown
```bash
cargo bench
```

```rust
fn example() {}
```
```

## Maintainer Review Rubric (lightweight)
Use these prompts for review; they guide judgment, not strict pass/fail.
- Does the page clearly communicate the value and outcome?
- Is the structure scannable with sensible headings and bullets?
- Do visuals (if any) include captions and add clarity?
- Is there a clear path for non-template projects (Integration or Quick Start)?
- Are links correct and helpful (internal relative, external official)?
- For technical pages: are deeper examples present where necessary, not everywhere?

Remember: Feature pages should showcase capabilities and enable adoption. Link to deep guides for comprehensive instruction where appropriate.
