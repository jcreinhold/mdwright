# Performance Workflow

Use this order. Do not skip ahead.

## 1. Define The Workload

Pick the workload that matches the user-visible complaint.

- Parser or syntax-only suspicion: use parser-only or parse-stage benches.
- Mid-pipeline suspicion (typecheck, analysis, normalization, traversal): start with the nearest crate bench, then
    confirm with a broader workload if the change may affect end-to-end latency.
- End-to-end throughput suspicion: use a pipeline or corpus bench, or a shared profiling binary if the project has one.
- Runtime, interpreter, or codegen suspicion: use the runtime/backend benches close to the symptom.

Prefer existing workloads over inventing new ones. If nothing credible exists, add one before changing code.

## 2. Establish A Baseline

Use optimized builds for performance claims.

```bash
cargo bench -p <crate> --bench <bench_name>
```

If the project has a shared profiling binary or workspace crate for end-to-end profiling, use it for broader claims:

```bash
export RUSTFLAGS="-C target-cpu=native -C force-frame-pointers=yes"
cargo run --profile profiling -p <profiling_crate> --bin <workload>
```

Pick the lightest workload that exercises the suspected hot path; reach for fuller workloads only when allocation
pressure or end-to-end latency claims are in question.

## 3. Localize The Bottleneck

Match tool to symptom.

- CPU time hot path: Criterion, `cargo flamegraph`, samply, or project profiling scripts.
- Allocation rate or retained heap suspicion: DHAT (`dhat-heap` feature in Criterion), allocation-aware baseline runs.
- Cache/layout suspicion: inspect sizes, pointer chasing, key choice, and hot/cold field mix after profiling points
    there.
- Compile-time code size suspicion in codegen-heavy crates: consider `cargo llvm-lines` only after runtime or throughput
    profiles point at LLVM/codegen work.

Useful Criterion commands:

```bash
cargo bench -p <crate> --bench <bench_name> -- --save-baseline before
cargo bench -p <crate> --bench <bench_name> -- --baseline before
cargo bench -p <crate> --bench <bench_name> -- --profile-time 10
```

Use `--profile-time` when attaching a profiler to Criterion benches so Criterion's own sampling logic does not dominate
the capture.

## 4. Choose The Intervention Level

Use this order unless the data clearly says otherwise.

1. Remove work: algorithm, invalidation, batching, deduplication, cache scope.
1. Fix representation: indexed handles, interning, side tables, borrow vs own, environment representation.
1. Fix allocation strategy: arena, scratch reuse, exact capacity, fewer transient vectors or maps.
1. Fix layout and locality: contiguous storage, smaller hot structs, hot/cold split, integer keys, branch shape.
1. Fix hashing or lookup strategy.
1. Only then consider low-level tuning, build flags, or backend-specific work.

## 5. Re-Measure

After the code change:

- Re-run the exact benchmark or profile used for the baseline.
- Re-run one broader workload that could regress elsewhere.
- If the change affects ownership, allocation, or caching, check both time and memory.

Evidence should include:

- workload name
- command
- before and after numbers
- whether caches were warm or cold
- whether the result is microbench-only or confirmed end-to-end

## 6. Stop Conditions

Stop and collect better data when:

- the measured change is small enough to be lost in noise
- the benchmark does not match the real workload
- the only visible win comes from changing setup code, not the hot path
- you can only show a microbench win for a change that obviously shifts work elsewhere
- you suspect the bottleneck moved but have not re-profiled

## Failure Smells

- "This should be faster" with no workload.
- Reporting only percent speedup with no raw numbers.
- Measuring debug builds.
- Benchmarking setup or construction when the complaint is hot-path work.
- Declaring victory from one bench while pipeline throughput or memory gets worse.
- Changing data structure, allocator, or hasher without characterizing keys, sizes, or lifetime.
