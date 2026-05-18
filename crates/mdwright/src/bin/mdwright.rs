//! Official `mdwright` binary.

fn main() -> std::process::ExitCode {
    mdwright::run_with_rules(mdwright_lint::stdlib::all())
}
