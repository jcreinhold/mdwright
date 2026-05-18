//! Official `mdwright` binary. Delivery lives in the CLI crate; the
//! root crate is a library facade for parser, formatter, linter, and
//! config users.

fn main() -> std::process::ExitCode {
    mdwright_cli::run_with_rules(mdwright_lint::stdlib::all())
}
