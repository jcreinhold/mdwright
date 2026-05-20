#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test harness; assertions surface as panics"
)]

//! Validate the JSON Lines v2 record shape from `mdwright check
//! --format=json`. Each line must parse, carry `schema_version: 2`,
//! and match the field set documented in `docs/diagnostic-schema.md`.

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

fn mdwright() -> &'static str {
    env!("CARGO_BIN_EXE_mdwright")
}

fn run_json(input: &str, fmt: &str) -> (String, String) {
    let mut child = Command::new(mdwright())
        .args(["check", &format!("--format={fmt}"), "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn mdwright");
    {
        use std::io::Write as _;
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(input.as_bytes()).expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait mdwright");
    (
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
    )
}

#[test]
fn v2_record_shape() {
    let (stdout, _stderr) = run_json("See https://example.com here.\n", "json");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(!lines.is_empty(), "expected at least one diagnostic line");

    for line in lines {
        let v: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("invalid JSON: {e}: `{line}`"));
        let obj = v.as_object().expect("record is an object");

        let keys: BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        // Required keys
        for k in ["schema_version", "path", "severity", "rule", "source", "message"] {
            assert!(keys.contains(k), "missing required key `{k}` in {line}");
        }
        // No unexpected top-level keys (fix is optional).
        for k in &keys {
            assert!(
                matches!(
                    *k,
                    "schema_version" | "path" | "severity" | "rule" | "source" | "message" | "fix"
                ),
                "unexpected key `{k}` in {line}"
            );
        }
        assert_eq!(obj["schema_version"], Value::from(2));
        let sev = obj["severity"].as_str().expect("severity is string");
        assert!(matches!(sev, "error" | "warning" | "advisory"), "bad severity `{sev}`");

        let rule = obj["rule"].as_object().expect("rule object");
        for k in ["name", "description", "url"] {
            assert!(rule.contains_key(k), "rule missing `{k}`");
        }
        let url = rule["url"].as_str().unwrap_or("");
        let looks_published = url.starts_with("https://")
            && url.contains("/rules/")
            && std::path::Path::new(url).extension().is_some_and(|e| e == "html");
        assert!(looks_published, "rule.url should be a published-site URL: {url}");

        let source = obj["source"].as_object().expect("source object");
        for k in ["line", "column", "span_start", "span_end", "snippet"] {
            assert!(source.contains_key(k), "source missing `{k}`");
        }
        assert!(source["line"].as_u64().unwrap_or(0) >= 1);
        assert!(source["column"].as_u64().unwrap_or(0) >= 1);
    }
}

#[test]
fn v1_still_emits_old_shape_with_deprecation_warning() {
    let (stdout, stderr) = run_json("See https://example.com here.\n", "json-v1");
    assert!(
        stderr.contains("--format=json-v1 is deprecated"),
        "missing deprecation warning on stderr: `{stderr}`"
    );
    let line = stdout.lines().next().expect("at least one record");
    let v: Value = serde_json::from_str(line).expect("v1 record is JSON");
    let obj = v.as_object().expect("v1 record is an object");
    // v1 has flat layout: path, line, column, span_start, span_end,
    // rule (as string), advisory, message, fix.
    for k in [
        "path",
        "line",
        "column",
        "span_start",
        "span_end",
        "rule",
        "advisory",
        "message",
    ] {
        assert!(obj.contains_key(k), "v1 missing `{k}`: {line}");
    }
    assert!(obj["rule"].is_string(), "v1 rule is a string");
    assert!(!obj.contains_key("schema_version"), "v1 must not carry schema_version");
}
