# Contributing to TokenScavenger

Thank you for your interest in contributing! This document outlines the process for contributing to TokenScavenger.

## Code of Conduct

All contributors must abide by our [Code of Conduct](CODE_OF_CONDUCT.md).

## How to Contribute

### Reporting Bugs

1. Check the [issue tracker](https://github.com/kabudu/token-scavenger/issues) to see if the bug is already reported.
2. If not, open a new issue with:
   - A clear title and description
   - Steps to reproduce
   - Expected vs actual behavior
   - Your environment (OS, Rust version, provider configuration)

### Suggesting Features

1. Open a feature request issue describing the proposed feature.
2. Explain the use case and why it benefits the project.
3. Check [ROADMAP.md](ROADMAP.md) to see whether the idea fits one of the current product directions.
4. If possible, outline how the feature might be implemented.

### Pull Requests

1. Fork the repository.
2. Create a feature branch: `git checkout -b feature/my-feature`.
3. Make your changes following the code style guidelines.
4. Add or update tests as needed.
5. Run the full test suite: `cargo fmt --all && cargo clippy --all-targets --all-features && cargo test --all-features`.
6. Ensure all tests pass and no new warnings are introduced.
7. Update relevant documentation and the `.dev/` implementation checklists.
8. Submit a pull request with a clear description of the changes.

For larger product changes, use [ROADMAP.md](ROADMAP.md) as the north star and keep each PR narrow enough to review safely.

## Development Setup

### Prerequisites

- Rust 1.85+ (edition 2024)
- SQLite development libraries (usually `libsqlite3-dev` on Linux, included on macOS)

### Building

```bash
cargo build
```

### Testing

```bash
# Run all tests (unit + integration)
cargo test

# Run specific test
cargo test test_healthz_returns_ok

# Run with output
cargo test -- --nocapture
```

### Code Style

- Follow standard Rust formatting: `cargo fmt --all`
- Clippy must pass with no warnings: `cargo clippy --all-targets --all-features`
- Use descriptive variable names and add doc comments for public API items
- Keep functions focused and reasonably sized

## Project Structure

```
src/
  api/          OpenAI-compatible routes, request/response types
  app/          Application state, startup/shutdown
  config/       Configuration loading and validation
  db/           Database connection and migrations
  discovery/    Model discovery and catalog management
  metrics/      Prometheus metrics and structured tracing
  providers/    Provider adapter implementations
  resilience/   Circuit breakers, health, retry logic
  router/       Route planning and execution
  ui/           Embedded operator web interface
  usage/        Usage accounting and cost estimation
  util/         Shared utilities (redaction, time helpers)
```

## Adding a New Provider

1. Create a new adapter module in `src/providers/` implementing `ProviderAdapter` trait.
2. If the provider is OpenAI-compatible, use the shared helpers in `src/providers/shared.rs`.
3. Register the adapter in `src/providers/registry.rs` (`create_adapter` function).
4. Add the provider ID to the default `provider_order` in `src/router/policy.rs`.
5. Add curated models in `src/discovery/curated.rs`.
6. Update `documentation/provider-matrix.md` with the new provider's details.
7. Add the provider ID to the `provider_order` defaults.

## Versioning

TokenScavenger follows [Semantic Versioning](https://semver.org/). Breaking changes to the OpenAI-compatible API surface, config schema, or data format require a major version bump.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
