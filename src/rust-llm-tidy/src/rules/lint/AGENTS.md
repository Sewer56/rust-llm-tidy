# Diagnostic guidance

Write diagnostics that help humans understand the finding and LLMs act safely.

## Message structure

- Lead with the concrete finding, including measured values when relevant.
- Include `Why:` and `Suggestions:` in every lint diagnostic; keep both concise.
- Use `Why:` for human-facing reasons: explain what makes code or docs harder
  for people to read, navigate, or understand. Avoid tool mechanics as the reason.
- Use `Suggestions:` for concrete actions an LLM could take. Treat judgment-based
  refactoring as an option, not a command to satisfy the lint mechanically.
- Use short bullets for separate suggestions and multi-part reasons.

## Safe guidance

- Preserve behavior, contracts, useful context, and performance. Include relevant
  safeguards against invented documentation, needless allocations, or wider APIs.
- Match severity to the finding. Suggestions requiring judgment belong at `Hint`;
  do not downgrade required contracts just because their messages offer guidance.

## Documentation and tests

- Update message tests and documented output examples together.
- Show diagnostic output in full, including both sections.
- Limit remarks to lint detection details, after examples and CLI output.
- Put reasons in `Why:`, not repeated documentation remarks.
