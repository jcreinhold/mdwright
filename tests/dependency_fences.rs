#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "dependency fence failures should panic with direct file/manifest context"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn repo_file(path: impl AsRef<Path>) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name == "target" || name == "book" {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
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
fn root_facade_does_not_depend_on_delivery() {
    assert_tree_excludes(
        "mdwright",
        &[
            "mdwright-cli",
            "mdwright-lsp",
            "clap ",
            "ignore ",
            "rayon ",
            "tokio ",
            "tower-lsp",
            "owo-colors",
            "anyhow ",
        ],
    );
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

#[test]
fn root_package_owns_no_binary_target() {
    let manifest = fs::read_to_string(repo_file("Cargo.toml")).expect("read root Cargo.toml");
    let value: toml::Value = toml::from_str(&manifest).expect("parse root Cargo.toml");
    assert!(
        value.get("bin").is_none(),
        "root mdwright package must not declare a binary target"
    );
    assert!(
        !repo_file("src/bin/mdwright.rs").exists(),
        "the mdwright executable belongs to mdwright-cli"
    );
}

#[test]
fn formatter_does_not_import_parser_or_pulldown() {
    let mut files = Vec::new();
    collect_files(&repo_file("crates/mdwright-format/src"), &mut files);
    for path in files {
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read formatter source");
        assert!(
            !text.contains("pulldown_cmark"),
            "{} must use document-owned facts, not pulldown directly",
            path.display()
        );
        let parse_path = ["mdwright_document", "::", "parse"].concat();
        assert!(
            !text.contains(&parse_path),
            "{} must not import the document parser chokepoint",
            path.display()
        );
    }
}

#[test]
fn document_parser_module_is_not_public() {
    let lib = fs::read_to_string(repo_file("crates/mdwright-document/src/lib.rs")).expect("read document lib");
    let public_parse_mod = ["pub", " mod ", "parse"].concat();
    assert!(
        !lib.contains(&public_parse_mod),
        "mdwright-document must not publicly export parser mechanics"
    );
}

#[test]
fn config_schema_uses_parse_extensions() {
    let mut files = Vec::new();
    for root in ["README.md", "docs/src", "crates", "tests", "examples"] {
        let path = repo_file(root);
        if path.is_dir() {
            collect_files(&path, &mut files);
        } else {
            files.push(path);
        }
    }
    let old_key = ["fmt", ".extensions"].concat();
    for path in files {
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if !matches!(ext, "rs" | "md" | "toml") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read source file");
        assert!(
            !text.contains(&old_key),
            "{} still refers to formatter-owned extension config",
            path.display()
        );
    }
}

#[test]
fn internal_workspace_dependencies_are_versioned_paths() {
    let manifest = fs::read_to_string(repo_file("Cargo.toml")).expect("read root Cargo.toml");
    let value: toml::Value = toml::from_str(&manifest).expect("parse root Cargo.toml");
    let workspace_deps = value
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
        .expect("workspace dependencies table");
    for name in [
        "mdwright-cli",
        "mdwright-config",
        "mdwright-document",
        "mdwright-format",
        "mdwright-lint",
        "mdwright-lsp",
        "mdwright-math",
    ] {
        let dep = workspace_deps
            .get(name)
            .and_then(toml::Value::as_table)
            .unwrap_or_else(|| {
                panic!("missing workspace dependency for {name}");
            });
        assert!(
            dep.contains_key("path"),
            "{name} must keep a local path for workspace development"
        );
        assert!(
            dep.contains_key("version"),
            "{name} must include a version requirement for cargo package"
        );
    }
}
