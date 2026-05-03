## Summary

<!-- What changed, and why? Keep this short and operator-focused. -->

## Type of Change

- [ ] Bug fix
- [ ] Feature
- [ ] Provider integration
- [ ] Documentation
- [ ] UI/UX
- [ ] Refactor or maintenance
- [ ] Release or CI

## Compatibility And Safety

- [ ] Preserves OpenAI-compatible behavior for supported endpoints.
- [ ] Does not introduce required runtime dependencies outside the single Rust binary and SQLite.
- [ ] Keeps provider-specific behavior inside provider adapters or shared provider utilities.
- [ ] Does not expose secrets in logs, UI, API responses, fixtures, or test output.
- [ ] Does not silently route to paid providers unless paid fallback policy allows it.

## Documentation

- [ ] Updated `README.md` if project behavior or positioning changed.
- [ ] Updated relevant files in `documentation/`.
- [ ] Updated relevant `.dev/implementation*.md` or forensic checklist items.
- [ ] Updated the marketing website in `docs/` if user-facing project messaging changed.
- [ ] Not applicable.

## Testing

Please list the checks you ran:

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features
cargo test --all-features
```

Additional checks:

```text

```

## Screenshots Or Logs

<!-- Add UI screenshots, route-plan output, provider logs, or release artifacts when useful. -->

## Notes For Reviewers

<!-- Call out migration impact, provider credentials needed for manual checks, follow-up work, or areas that deserve extra attention. -->
