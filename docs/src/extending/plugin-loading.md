# Plugin loading

> **Status:** Design phase. Tracked under prompt 25 in the project roadmap; not implemented in 0.3.0. This page
> describes the contract that will land.

mdwright today links lint rules at compile time: the stdlib registers in `RuleSet::stdlib_all()`, and third-party rules
require a custom mdwright binary. Plugin loading will let a project ship its own rules as a separate crate and have the
standard `mdwright` binary pick them up at runtime.

The design constraint is that plugins are pure Rust and load in-process — no JavaScript bridge, no WASM sandbox, no
IPC. The mechanism will be a stable `extern "Rust"` factory function in a dynamic library that mdwright loads from a
configured path. Concretely, a plugin will:

1. Export `fn mdwright_register(reg: &mut Registry)`.
2. Pin a specific `mdwright-plugin-api` semver-major version.
3. Ship as a `cdylib` next to the project's `.mdwright.toml`.

The configuration entry will look like:

```toml,no-check
[lint]
plugins = ["./target/debug/libcorporate_lints.dylib"]
```

Watch the [issue tracker](https://github.com/jcreinhold/mdwright/issues) for the API freeze.

Until then, the only path is compile-time linkage; see [Lint rules](lint-rules.md).
