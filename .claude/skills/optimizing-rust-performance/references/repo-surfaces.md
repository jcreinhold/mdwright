# Repo Measurement Surfaces

A real Rust project usually has more measurement support than its README advertises. Inventory what already exists
before adding new infrastructure.

## Criterion Benches

Look first under `crates/*/benches/*.rs`. These typically split into:

- **Per-crate microbenches** — single-function or single-stage benches that pin one hot path. Use them to localize.
- **Pipeline/throughput benches** — exercise multiple stages end-to-end. Use them to confirm an end-to-end claim.
- **Regression benches** — guard specific commits or commit ranges from re-regressing.

Discovery commands:

```bash
# List every bench file in the workspace.
rg --files | rg '/benches/'

# Find which crate owns each bench manifest entry.
rg -n '^\[\[bench\]\]' crates/*/Cargo.toml
```

## Shared Profiling Surface

A workspace may also have a dedicated profiling crate or scripts (often under `profiling/`, `xtask/`, or `scripts/`).
Conventional shape:

- `bin/collect_baseline_*.rs` — timing and allocation baselines suitable for committing as evidence.
- `bin/profile_*.rs` — flamegraph / pprof / DHAT captures bound to a named workload.
- `scripts/profile.sh`, `scripts/profile_with_samply.sh` — wrappers that fix RUSTFLAGS and output locations.

Discovery commands:

```bash
# Top-level profiling artifacts.
ls profiling/ xtask/src/bin/ scripts/ 2>/dev/null

# Existing in-source profiling hooks.
rg -n 'profiling|with_profiling_observer|ProfilerGuard|dhat|criterion_group' .
```

## When To Add Measurement Support

Add or expand a bench or profiling surface when:

- the suspected hot path has no stable reproducer
- the only current bench measures the wrong thing, such as setup instead of steady-state work
- a change touches a pass boundary, cache, or invalidation policy that microbenches cannot cover
- reviewers would otherwise have no credible way to detect regressions later

Bias toward existing surfaces. A bench added near sibling benches inherits their CI integration and reviewer attention;
a one-off bench dropped at the workspace root usually does not.
