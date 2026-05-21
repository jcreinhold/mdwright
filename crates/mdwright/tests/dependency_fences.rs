#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "dependency fence failures should panic with direct file/manifest context"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/mdwright has a workspace root")
        .to_owned()
}

fn repo_file(path: impl AsRef<Path>) -> PathBuf {
    repo_root().join(path)
}

fn cargo_tree(package: &str) -> String {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .current_dir(repo_root())
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

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if matches!(name, ".git" | "target" | "book") {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn workspace_metadata() -> serde_json::Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .current_dir(repo_root())
        .args(["metadata", "--no-deps", "--format-version=1"])
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("metadata is json")
}

#[test]
fn root_manifest_is_virtual_workspace() {
    let manifest = fs::read_to_string(repo_file("Cargo.toml")).expect("read root Cargo.toml");
    let value: toml::Value = toml::from_str(&manifest).expect("parse root Cargo.toml");
    assert!(
        value.get("workspace").is_some(),
        "root manifest must define the workspace"
    );
    assert!(
        value.get("package").is_none(),
        "root manifest must be virtual, not a facade package"
    );
    assert!(
        !repo_file("src/lib.rs").exists(),
        "root must not contain a facade library"
    );
}

#[test]
fn no_removed_cli_package_exists() {
    let removed_package = ["mdwright", "-cli"].concat();
    let metadata = workspace_metadata();
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .expect("packages array");
    assert!(
        packages
            .iter()
            .all(|pkg| pkg.get("name").and_then(serde_json::Value::as_str) != Some(removed_package.as_str())),
        "the command package is named mdwright, not {removed_package}"
    );
}

#[test]
fn mdwright_package_owns_mdwright_binary() {
    let metadata = workspace_metadata();
    let package = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .and_then(|packages| {
            packages
                .iter()
                .find(|pkg| pkg.get("name").and_then(serde_json::Value::as_str) == Some("mdwright"))
        })
        .expect("mdwright package exists");
    let targets = package
        .get("targets")
        .and_then(serde_json::Value::as_array)
        .expect("targets array");
    let has_binary = targets.iter().any(|target| {
        target.get("name").and_then(serde_json::Value::as_str) == Some("mdwright")
            && target
                .get("kind")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("bin")))
    });
    assert!(has_binary, "package mdwright must own the mdwright binary");
}

#[test]
fn command_library_is_not_a_facade() {
    let lib = fs::read_to_string(repo_file("crates/mdwright/src/lib.rs")).expect("read command lib");
    for forbidden in [
        "pub use mdwright_config",
        "pub use mdwright_document",
        "pub use mdwright_format",
        "pub use mdwright_lint",
    ] {
        assert!(
            !lib.contains(forbidden),
            "command helper library must not recreate the deleted facade via `{forbidden}`"
        );
    }
}

#[test]
fn old_cli_package_name_is_gone_from_sources() {
    let mut files = Vec::new();
    for root in [
        ".cargo",
        ".pre-commit-hooks.yaml",
        "Cargo.toml",
        "README.md",
        "action.yml",
        "crates",
        "docs/src",
        "examples",
        "fuzz/Cargo.toml",
        "xtask",
    ] {
        let path = repo_file(root);
        if path.is_dir() {
            collect_files(&path, &mut files);
        } else {
            files.push(path);
        }
    }
    let removed_package = ["mdwright", "-cli"].concat();
    let removed_crate = ["mdwright", "_cli"].concat();
    for path in files {
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if !matches!(ext, "rs" | "md" | "toml" | "yaml" | "yml") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("read source file");
        assert!(
            !text.contains(&removed_package) && !text.contains(&removed_crate),
            "{} still refers to the removed {removed_package} package",
            path.display()
        );
    }
}

#[test]
fn latex_has_no_mdwright_dependencies() {
    let tree = cargo_tree("mdwright-latex");
    let deps = tree.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert!(
        !deps.contains("mdwright-") && !deps.contains("mdwright v"),
        "mdwright-latex must not depend on another mdwright crate\n{tree}"
    );
}

#[test]
fn math_depends_only_on_latex_boundary() {
    let tree = cargo_tree("mdwright-math");
    for line in tree.lines().skip(1) {
        if line.contains("mdwright-") || line.contains("mdwright v") {
            assert!(
                line.contains("mdwright-latex"),
                "mdwright-math may depend on mdwright-latex only, not another mdwright crate\n{tree}"
            );
        }
    }
}

#[test]
fn math_does_not_reexport_latex_as_a_facade() {
    let lib = fs::read_to_string(repo_file("crates/mdwright-math/src/lib.rs")).expect("read math lib");
    assert!(
        !lib.contains("pub use mdwright_latex"),
        "mdwright-math must not re-export mdwright-latex as a pass-through facade"
    );
}

#[test]
fn terminal_math_rendering_is_first_party() {
    assert_tree_excludes("mdwright", &["term-maths", "tui-math"]);
    assert_tree_excludes("mdwright-latex", &["term-maths", "tui-math"]);
}

#[test]
fn latex_boundary_has_no_delivery_or_markdown_dependencies() {
    assert_tree_excludes(
        "mdwright-latex",
        &[
            "mdwright-math",
            "mdwright-document",
            "mdwright-format",
            "mdwright-lint",
            "mdwright-config",
            "mdwright-lsp",
            "pulldown-cmark",
            "syntect",
            "opener",
            "clap ",
            "ignore ",
            "rayon ",
            "tokio ",
            "tower-lsp",
        ],
    );
}

#[test]
fn terminal_preview_delivery_dependencies_stay_in_command_package() {
    for package in [
        "mdwright-latex",
        "mdwright-math",
        "mdwright-document",
        "mdwright-format",
        "mdwright-lint",
        "mdwright-config",
        "mdwright-lsp",
    ] {
        assert_tree_excludes(package, &["syntect", "opener"]);
    }
}

#[test]
fn document_depends_only_downward() {
    assert_tree_excludes(
        "mdwright-document",
        &[
            "mdwright-format",
            "mdwright-lint",
            "mdwright-config",
            "mdwright v",
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
            "mdwright v",
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
            "mdwright v",
            "mdwright-lsp",
            "clap ",
            "tokio ",
            "tower-lsp",
        ],
    );
}

#[test]
fn config_does_not_depend_on_delivery() {
    assert_tree_excludes("mdwright-config", &["mdwright v", "mdwright-lsp"]);
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
fn document_format_facts_do_not_reparse_for_accessors() {
    let source = fs::read_to_string(repo_file("crates/mdwright-document/src/format_facts.rs"))
        .expect("read document format facts");
    let parser_calls = source.matches("parse::collect_events_with_offsets").count();
    assert_eq!(
        parser_calls, 0,
        "document formatter facts must be cached from Ir::parse and must not reparse source"
    );
    assert!(
        !source.contains("unwrap_or_default"),
        "document fact accessors must not hide parser failures by returning empty facts"
    );
}

#[test]
fn lsp_test_service_helper_is_not_public() {
    let lib = fs::read_to_string(repo_file("crates/mdwright-lsp/src/lib.rs")).expect("read lsp lib");
    assert!(
        !lib.contains("build_service_for_tests"),
        "mdwright-lsp must not export test-only service constructors"
    );
    let lsp = fs::read_to_string(repo_file("crates/mdwright-lsp/src/lsp.rs")).expect("read lsp implementation");
    assert!(
        !lsp.contains("pub fn build_service_for_tests"),
        "test-only service constructor must stay crate-private"
    );
}

#[test]
fn document_does_not_reexport_deleted_helpers() {
    let lib = fs::read_to_string(repo_file("crates/mdwright-document/src/lib.rs")).expect("read document lib");
    for forbidden in [
        "find_attr_trailer_range",
        "NormalisedLabel",
        "CanonicalSource",
        "OffsetMap",
        "Source",
        "ByteSpan",
        "NodeKind",
        "Tree",
        "top_level_block_checkpoints",
        "UnorderedListSite",
        "OrderedListSite",
        "OrderedItemSite",
        "InlineDelimiterSpan",
        "InlineLinkDestinationSite",
    ] {
        assert!(
            !lib.contains(forbidden),
            "mdwright-document must not re-export deleted helper `{forbidden}`"
        );
    }
}

#[test]
fn formatter_inline_rewrites_use_slot_facts() {
    let canonicalise =
        fs::read_to_string(repo_file("crates/mdwright-format/src/format/canonicalise.rs")).expect("read canonicalise");
    for forbidden in [
        "inline_delimiter_spans",
        "inline_link_destination_sites",
        "open_lo..close_hi",
        "open_hi..close_lo",
    ] {
        assert!(
            !canonicalise.contains(forbidden),
            "formatter inline canonicalisation must use document-owned slots, not broad inline ranges: `{forbidden}`"
        );
    }
}

#[test]
fn config_schema_uses_parse_extensions() {
    let mut files = Vec::new();
    for root in ["README.md", "docs/src", "crates", "examples"] {
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
fn fuzz_artifact_directories_are_clean() {
    let artifact_root = repo_file("fuzz/artifacts");
    let mut files = Vec::new();
    collect_files(&artifact_root, &mut files);
    files.retain(|path| path.is_file());
    assert!(
        files.is_empty(),
        "fuzz artifacts must be diagnosed, minimised into regressions, or deleted before commit:\n{}",
        files
            .iter()
            .map(|path| format!("  {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
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
        "mdwright",
        "mdwright-config",
        "mdwright-document",
        "mdwright-format",
        "mdwright-latex",
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
