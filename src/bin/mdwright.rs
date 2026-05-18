//! Official `mdwright` binary. Delegates everything to
//! [`mdwright::cli::run_with_rules`] with the stdlib rule set.
//! Downstream binaries that want to register extra rules call the
//! same function with their augmented [`mdwright::RuleSet`]; see
//! `examples/extending/` and `docs/src/extending/lint-rules.md`.

fn main() -> std::process::ExitCode {
    mdwright::cli::run_with_rules(mdwright::stdlib::all())
}
