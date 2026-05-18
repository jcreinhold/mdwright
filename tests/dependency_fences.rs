use std::process::Command;

#[allow(
    clippy::expect_used,
    reason = "dependency fence failures should panic with the command context"
)]
fn cargo_tree(package: &str) -> String {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args(["tree", "-e", "normal", "-p", package])
        .output()
        .expect("cargo tree runs");
    assert!(
        output.status.success(),
        "cargo tree failed for {package}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("cargo tree output is utf-8")
}

fn assert_tree_excludes(package: &str, banned: &[&str]) {
    let tree = cargo_tree(package);
    for banned_name in banned {
        assert!(
            !tree.contains(banned_name),
            "{package} must not depend on {banned_name}\n{tree}"
        );
    }
}

#[test]
fn math_has_no_mdwright_dependencies() {
    let tree = cargo_tree("mdwright-math");
    let deps = tree.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert!(
        !deps.contains("mdwright-"),
        "mdwright-math must not depend on another mdwright crate\n{tree}"
    );
}

#[test]
fn document_depends_only_downward() {
    assert_tree_excludes(
        "mdwright-document",
        &[
            "mdwright-format",
            "mdwright-lint",
            "mdwright-config",
            "mdwright-cli",
            "mdwright-lsp",
            "clap ",
            "ignore ",
            "rayon ",
            "serde ",
            "toml ",
            "tokio ",
            "tower-lsp",
            "owo-colors",
            "anyhow ",
        ],
    );
}

#[test]
fn formatter_does_not_depend_on_delivery_or_lint() {
    assert_tree_excludes(
        "mdwright-format",
        &[
            "mdwright-lint",
            "mdwright-cli",
            "mdwright-lsp",
            "clap ",
            "tokio ",
            "tower-lsp",
        ],
    );
}

#[test]
fn linter_does_not_depend_on_formatter_or_delivery() {
    assert_tree_excludes(
        "mdwright-lint",
        &[
            "mdwright-format",
            "mdwright-cli",
            "mdwright-lsp",
            "clap ",
            "tokio ",
            "tower-lsp",
        ],
    );
}

#[test]
fn config_does_not_depend_on_delivery() {
    assert_tree_excludes("mdwright-config", &["mdwright-cli", "mdwright-lsp"]);
}

#[test]
fn root_facade_contains_no_implementation_modules() {
    for path in [
        "src/document.rs",
        "src/format",
        "src/ir.rs",
        "src/tree.rs",
        "src/config.rs",
        "src/diagnostic.rs",
        "src/rule.rs",
        "src/rule_set.rs",
        "src/stdlib",
        "src/lsp.rs",
        "src/cli.rs",
    ] {
        assert!(
            !std::path::Path::new(path).exists(),
            "{path} should live in an owner crate"
        );
    }
}
