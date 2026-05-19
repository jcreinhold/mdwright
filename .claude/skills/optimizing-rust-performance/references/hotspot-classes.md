# Common Hotspot Classes

Bottleneck classes that recur across performance-sensitive Rust codebases. Use these to localize quickly; replace the
generic examples with your own bench paths once you know the project.

## Arena And Phase-Local Allocation

Start here when you see many short-lived vectors, slices, or strings.

What to look for:

- building temporary `Vec`s only to copy into arena slices
- re-allocating scratch buffers inside loops instead of clearing and reusing
- storing data past a phase boundary when it should die with the arena
- using store/load or owned clones as a borrow workaround instead of fixing lifetime structure

## Hot-Path Computation (Parse, Normalize, Evaluate, Read-Back)

The dominant CPU paths in compiler-shaped or parser-shaped tools.

Typical bottlenecks:

- repeated computation that a cached or incremental fact would replace
- unnecessary full processing when a lighter pass would suffice
- environment growth or copying under nested scopes
- recursive or pointer-heavy traversals in hot steady-state loops

## Traversal Cost

Traversal shows up both directly and as overhead inside other passes.

Discovery:

```bash
rg -n "fold|walk|visitor|travers" crates
```

Typical bottlenecks:

- repeated full traversals where a cached or incremental fact would do
- collecting transient child vectors instead of using an explicit stack or iterator
- recomputing structure facts on every pass

## Solvers, Constraint Queues, Resolution Tables

For projects with constraint-solving, type-inference, or unifier-like components.

Typical bottlenecks:

- repeated normalization during dispatch
- constraint deduplication or wake-up overhead
- solution-lookup churn
- cloning definitions, environments, or metadata at retry boundaries
- persistent-structure costs in deep or write-heavy paths

## Registry, Metadata, Side Tables, Cache Lookups

These costs often hide behind "small" operations executed everywhere.

Typical bottlenecks:

- `HashMap<Vec<PathSegment>, ...>`-style keys on hot paths
- repeated registry cloning or cache-key cloning across phase boundaries
- converting cheap IDs into expensive path or string keys too early
- cold metadata bloating hot structs instead of living in a side table

## Closure Capture And Environment Representation

When the project uses closures or environments in multiple layers.

Discovery:

```bash
rg -n "CapturedEnv|closure|captured|imbl::Vector|push_back" crates
```

Typical bottlenecks:

- persistent vectors that help lookup but hurt repeated writes
- copying captured environments at instantiation or boundary crossings
- storing more in every closure than the hot path actually needs

## Pipeline Throughput And Pass Boundaries

Micro wins can lose here. Treat pipeline-throughput benches as a guardrail.

Typical bottlenecks:

- pass-local clones that scale with input count
- repeated arena setup or source-map allocation
- invalidation that forces broad recomputation
- data converted to a new representation at each pass when a stable handle would do

## Persistent Structures And Immutable Data

Persistent collections are not free. They help when sharing dominates copying, but they hurt when mutation depth
dominates.

Discovery:

```bash
rg -n "imbl::|Vector<|HashMap<|HashSet<|SmallVec<" crates
```

Questions to ask:

- Is the hot operation mostly reads, appends, random lookups, or full clones?
- Is structural sharing paying for itself?
- Would an arena slice, dense index storage, or reusable `Vec` be cheaper for this phase?
