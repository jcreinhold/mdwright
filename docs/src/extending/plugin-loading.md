# Plugin loading

mdwright does not load lint rules at runtime. The supported
extension path is the one [Writing a lint rule](lint-rules.md)
describes: depend on `mdwright-document` and `mdwright-lint`, implement `LintRule`,
call `mdwright::run_with_rules`, and ship your own binary.
This page explains why: what dynamic loading would buy, what it
would cost, and what would have to change for the decision to flip.

## The decision

| Architecture                                          | Verdict   | Available in |
| ----------------------------------------------------- | --------- | ------------ |
| **A.** Component crates + custom binary               | Supported | today        |
| **B.** Dynamic `cdylib` loading via `libloading`      | Rejected  | never        |
| **C.** WASM plugins via `wasmtime`                    | Deferred  | Phase 5+     |

The same trio shipped in `ruff`, which is mdwright's closest analogue
in spirit. ruff thrives without a plugin runtime; the trait surface
plus a documented "ship your own binary" path covers every adopter
who hits the limits of the stdlib.

## Architecture A: Supported

A user writes a rule in their own crate, depends on `mdwright-document`, `mdwright-lint`, and `mdwright` from
crates.io, and ships a small binary:

```rust,no_run
use mdwright_lint::stdlib;
# struct MyRule;
# impl mdwright_lint::LintRule for MyRule {
#     fn name(&self) -> &str { "my-rule" }
#     fn description(&self) -> &str { "" }
#     fn check(&self, _: &mdwright_document::Document, _: &mut Vec<mdwright_lint::Diagnostic>) {}
# }

fn main() -> std::process::ExitCode {
    let mut rules = stdlib::all();
    rules.add(Box::new(MyRule)).expect("unique name");
    mdwright::run_with_rules(rules)
}
```

|                         |                                              |
| ----------------------- | -------------------------------------------- |
| **Capability**          | Full library access. Any rule the stdlib could write, an external rule can write. |
| **Complexity**          | One CLI-crate function (`run_with_rules`). The rest is the trait that already shipped. |
| **Cost to user**        | They ship a Rust binary. CI needs `cargo`. They pin a major version of `mdwright`. |
| **Cost to maintainer**  | None new. The `LintRule` trait and the `mdwright::run_with_rules` signature are the surface; semver protects them. |
| **Semver implications** | `LintRule` is `1.0`-grade. `cli::run_with_rules` is a `fn(RuleSet) -> ExitCode`; that signature is stable. |

This is what mdwright ships, in the `mdwright` crate and the
`examples/extending/` workspace member.

## Architecture B: Dynamic Loading via `libloading` (Rejected)

`.mdwright.toml`:

```toml,no-check
[plugins]
my_rules = "./target/release/libmy_rules.dylib"
```

mdwright would load each `cdylib` at startup and look up a
`extern "Rust" fn mdwright_register(&mut Registry)` symbol.

|                         |                                              |
| ----------------------- | -------------------------------------------- |
| **Capability**          | Anything Rust can express.                   |
| **Complexity**          | A `libloading` integration, a `Registry` shim, a plugin ABI versioning story. |
| **Cost to user**        | They build a `cdylib` and put it in a path. First-run UX is opaque when the path is wrong. |
| **Cost to maintainer**  | Substantial. The ABI surface is every type a plugin touches, including `Diagnostic`, `Document`, and every accessor. Rust has no stable ABI. Every Rust release risks breaking every plugin. |
| **Semver implications** | `repr(Rust)` types cross the boundary; layout is unspecified. Every release becomes an ABI compatibility check. |

**Verdict:** rejected. The maintenance burden is high, the gain over
Architecture A is small (a `cargo build` versus an in-process load),
and Rust's lack of a stable ABI makes the contract perpetually
fragile. Linking a single `cdylib` into the official binary buys
*nothing* a custom binary doesn't already give you.

## Architecture C: WASM via `wasmtime` (Deferred)

`.mdwright.toml`:

```toml,no-check
[plugins]
my_rules = "./my-rules.wasm"
```

The plugin compiles to WebAssembly; mdwright runs it in a `wasmtime`
sandbox, serialising documents and diagnostics across the boundary.

|                         |                                              |
| ----------------------- | -------------------------------------------- |
| **Capability**          | Restricted to whatever API mdwright exposes through the host bindings. |
| **Complexity**          | Define and document a sandbox API; write host bindings; serialise `Document` and `Diagnostic` (no zero-copy across the boundary); manage WASM startup cost per file. |
| **Cost to user**        | Plugin authors learn `wasm-bindgen`-style discipline; the trait is harder to use than the native one. |
| **Cost to maintainer**  | Maintain the WASM API forever, plus a reference implementation, plus a performance story (parsing each file twice, once natively and once through the boundary, is not free). |
| **Semver implications** | The WASM API is its own semver surface, parallel to the native `LintRule` trait. |

**Verdict:** deferred to Phase 5+. The cost is real and the demand
is hypothetical. We will revisit when a concrete adopter has tried
Architecture A, hit a specific limit (sandbox isolation, language
diversity, hot reload), and articulated what the WASM contract would
need to address.

## What would change this decision

Architecture B is unlikely to ever become attractive: Rust would have
to grow a stable ABI, which is not on any horizon, and even then the
cost-benefit against custom binaries would barely move.

Architecture C is more interesting in principle. If you have a use
case where:

- the rule body is in a language other than Rust, or
- you need to load and unload rules without restarting `mdwright`, or
- a sandboxed evaluation model is a hard requirement (e.g. running
    rules submitted by untrusted contributors),

please [open an issue](https://github.com/jcreinhold/mdwright/issues)
describing the concrete adoption story. A motivated maintainer behind
a real need is the precondition for revisiting this.

Until then: depend on the library, write the rule, ship a binary.
The example at `examples/extending/` in the repository is ready to
fork.
