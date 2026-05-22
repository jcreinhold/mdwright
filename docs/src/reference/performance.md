# Performance

mdwright is parallel by default (rayon over the file walk) and free of per-file interpreter startup. On a
multi-thousand-file corpus the practical speedup over `mdformat --check` is the dominant cost difference; on small
inputs both tools are sub-second and the comparison is dominated by process startup.

## Measurement

| Tool | Wall time | Notes |
| --- | --- | --- |
| `mdwright fmt-check` | 108 ms ± 4 ms | Release build, rayon over 79 files. |
| `mdformat --check` | 5.91 s ± 0.59 s | Default install, single-threaded. |

Reproducer:

```sh
hyperfine --warmup 2 --runs 7 -N -i \
  './target/release/mdwright fmt-check <corpus>' \
  'mdformat --check <corpus>'
```

- **Corpus:** 79 Markdown files, ~34.5k lines of math-heavy technical prose (a checkout of `gentle-sga`).
- **Host:** Apple M4 Pro, macOS 26.4.1.
- **Versions:** `mdwright` from this workspace, release profile; `mdformat 0.7` (default plugins).
- **Result:** 55× ± 6×. The lede claim of "≥ 50× faster" is the floor of this measurement.

## What changes the multiplier

- **File count.** mdwright's startup is fixed; mdformat re-pays interpreter cost per file when invoked per-file. On
  directory invocations both tools amortise startup over the walk, but mdformat still single-threads the loop.
- **Core count.** mdwright scales with rayon's thread pool. On a single-core machine the multiplier drops; on a 16-core
  CI runner with a large corpus it climbs.
- **File size.** Per-byte parse cost is closer than the wall-time ratio suggests; on a single very large file, the ratio
  approaches the per-byte ratio rather than the per-file ratio.

## Reproducing locally

The bench harness used in development is Criterion, not hyperfine. See `crates/mdwright/benches/README.md` for
`cargo bench` recipes and corpus configuration. The hyperfine command above is the end-to-end smoke test; the Criterion
benches isolate parse, lint, and format costs separately.
