# Repo Patterns

## Preferred verification commands

Use `cargo nextest run`, not `cargo test`.

Common commands:

```bash
cargo nextest run -p <crate>
cargo nextest run -p <crate> --test <suite>
cargo nextest run --workspace
```

## Common test locations

- integration tests: `tests/` or `tests/it/`
- unit tests: inline `mod tests`
- benches: `benches/`

## Common test shapes

- core algorithms / data structures: laws, boundary cases, negative theory boundaries
- registry / storage: roundtrip, ordering, identity, conflict detection
- pipeline passes: preservation and semantic equivalence
- CLI / tooling: visible behavior, exit status, persisted state

## Prefer nearby authorities

Before writing tests, inspect:

- nearby docs/spec or architecture notes
- nearby issue or regression context
- existing tests in the same crate
- existing benches before inventing a new perf surface
