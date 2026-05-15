# mdwright benches

Criterion benches that measure parse and lint cost for the `mdwright` crate. Run them before claiming a perf delta.

## What is measured

- `parse/{small, medium}` — `Document::parse` only, single file.
- `lint/{defaults, all}/{small, medium}` — lint rule loop, with parse hoisted outside the timed region.
- `parse_plus_lint/defaults/{small, medium}` — full pipeline.
- `corpus/{none, defaults, all}` — rayon-parallel sweep over every file in `corpus.list`. File contents are pre-loaded
    so the benchmark times CPU work, not disk I/O.

Fixtures live in `fixtures/`:

- `small.md` — `docs/books/gentle-sga/i/appendix-black-boxes.md` (~75 lines).
- `medium.md` — `docs/books/gentle-sga/i/01-prerequisites-…md` (~960 lines; largest single file in the tree).

`corpus.list` lists every Markdown file under `docs/books/gentle-sga/i/`. Refresh it only when files are added or
removed:

```sh
find docs/books/gentle-sga/i -name '*.md' | sort \
    > tools/mdwright/benches/corpus.list
```

## Running

```sh
# Smoke (compiles + one iter per bench):
cargo bench --manifest-path tools/mdwright/Cargo.toml -- --quick

# Full run:
cargo bench --manifest-path tools/mdwright/Cargo.toml

# Save a named baseline:
cargo bench --manifest-path tools/mdwright/Cargo.toml \
    -- --save-baseline phase2

# Compare a later run against a baseline:
cargo bench --manifest-path tools/mdwright/Cargo.toml \
    -- --baseline phase2
```

Criterion writes results and an HTML report to `tools/mdwright/target/criterion/`. Open
`target/criterion/report/index.html` after a run.

## Baselines are local

Baselines are not committed. They depend on the host machine, thermal state, and rayon thread count. The convention:

- Capture a fresh `phase2` baseline on your machine before any optimisation work that claims to beat phase 2.
- Save a new named baseline after each intentional perf change (e.g. `--save-baseline phase3-formatter`).

The phrasing "no change vs phase2" is meaningful only relative to a baseline you captured on the same host in the recent
past.
