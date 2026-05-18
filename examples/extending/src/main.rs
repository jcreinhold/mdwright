//! Custom mdwright binary that registers one extra lint rule
//! (`no-todo-in-prose`) on top of the stdlib. Everything else —
//! `--rules` selection, `mdwright fmt`, `mdwright lsp`, output
//! formats, suppression comments — is inherited from
//! [`mdwright_cli::run_with_rules`] for free.

mod no_todo;

use mdwright::stdlib;

use crate::no_todo::NoTodoInProse;

fn main() -> std::process::ExitCode {
    let mut rules = stdlib::all();
    if let Err(err) = rules.add(Box::new(NoTodoInProse)) {
        eprintln!("mdwright-extra-example: failed to register rule: {err}");
        return std::process::ExitCode::from(2);
    }
    mdwright_cli::run_with_rules(rules)
}
