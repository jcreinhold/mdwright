#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration tests: panic / unwrap on missing values is the desired failure mode"
)]

//! Smoke tests for the in-tree LSP server.
//!
//! Drives `LspService` directly through `tower::Service` rather than
//! framing JSON-RPC over a pipe, so failures point at the handler that
//! misbehaved rather than at the transport. The published-diagnostics
//! path is exercised by draining the `ClientSocket` stream, which is
//! what real clients see.

use std::time::Duration;

use crate::lsp::build_service_for_tests;
use futures::StreamExt;
use serde_json::{Value, json};
use tower::{Service, ServiceExt};
use tower_lsp::jsonrpc::Request;
use tower_lsp::lsp_types::Url;

fn req(id: i64, method: &'static str, params: Value) -> Request {
    Request::build(method).id(id).params(params).finish()
}

fn notif(method: &'static str, params: Value) -> Request {
    Request::build(method).params(params).finish()
}

fn init_params(utf8: bool, root_uri: Option<&str>) -> Value {
    let position_encodings: Vec<&str> = if utf8 { vec!["utf-8"] } else { vec!["utf-16"] };
    json!({
        "capabilities": {
            "general": {
                "positionEncodings": position_encodings,
            },
            "textDocument": {
                "publishDiagnostics": {},
            },
        },
        "processId": null,
        "rootUri": root_uri,
    })
}

async fn initialize(
    service: &mut tower_lsp::LspService<impl tower_lsp::LanguageServer + 'static>,
    utf8: bool,
) -> Value {
    initialize_with_root(service, utf8, None).await
}

async fn initialize_with_root(
    service: &mut tower_lsp::LspService<impl tower_lsp::LanguageServer + 'static>,
    utf8: bool,
    root_uri: Option<&str>,
) -> Value {
    let resp = service
        .ready()
        .await
        .expect("service ready")
        .call(req(1, "initialize", init_params(utf8, root_uri)))
        .await
        .expect("call ok")
        .expect("initialize returns a response");
    let (_, body) = resp.into_parts();
    let body = body.expect("initialize result is Ok");
    let _ack = service
        .ready()
        .await
        .expect("service ready")
        .call(notif("initialized", json!({})))
        .await
        .expect("call ok");
    body
}

async fn open_doc(
    service: &mut tower_lsp::LspService<impl tower_lsp::LanguageServer + 'static>,
    uri: &str,
    version: i32,
    source: &str,
) {
    let _ack = service
        .ready()
        .await
        .expect("service ready")
        .call(notif(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "markdown",
                    "version": version,
                    "text": source,
                }
            }),
        ))
        .await
        .expect("call ok");
}

async fn change_doc(
    service: &mut tower_lsp::LspService<impl tower_lsp::LanguageServer + 'static>,
    uri: &str,
    version: i32,
    source: &str,
) {
    let _ack = service
        .ready()
        .await
        .expect("service ready")
        .call(notif(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": source }],
            }),
        ))
        .await
        .expect("call ok");
}

async fn request(
    service: &mut tower_lsp::LspService<impl tower_lsp::LanguageServer + 'static>,
    id: i64,
    method: &'static str,
    params: Value,
) -> Value {
    let resp = service
        .ready()
        .await
        .expect("service ready")
        .call(req(id, method, params))
        .await
        .expect("call ok")
        .unwrap_or_else(|| panic!("{method} returns a response"));
    let (_, body) = resp.into_parts();
    body.unwrap_or_else(|err| panic!("{method} returned error: {err:?}"))
}

#[tokio::test]
async fn initialize_returns_expected_capabilities() {
    let (mut service, _socket) = build_service_for_tests();
    let body = initialize(&mut service, true).await;
    let caps = &body["capabilities"];
    assert_eq!(caps["positionEncoding"], "utf-8", "utf-8 negotiated");
    assert_eq!(caps["textDocumentSync"], 1, "TextDocumentSyncKind::FULL");
    assert_eq!(caps["documentFormattingProvider"], true, "formatting advertised");
    assert_eq!(
        caps["documentRangeFormattingProvider"], true,
        "range formatting advertised"
    );
    assert_eq!(caps["hoverProvider"], true, "hover advertised");
    assert!(
        caps["codeActionProvider"].is_object(),
        "code actions advertised: {caps}"
    );
    let kinds = &caps["codeActionProvider"]["codeActionKinds"];
    assert!(
        kinds.as_array().is_some_and(|a| a.iter().any(|v| v == "quickfix")),
        "quickfix listed"
    );
    assert!(
        kinds.as_array().is_some_and(|a| a.iter().any(|v| v == "source.fixAll")),
        "fixAll listed",
    );
}

#[tokio::test]
async fn initialize_without_utf8_withdraws_formatting() {
    let (mut service, _socket) = build_service_for_tests();
    let body = initialize(&mut service, false).await;
    let caps = &body["capabilities"];
    assert!(
        caps.get("positionEncoding").is_none() || caps["positionEncoding"].is_null(),
        "no encoding advertised when client lacks UTF-8",
    );
    assert!(
        caps.get("documentFormattingProvider").is_none() || caps["documentFormattingProvider"].is_null(),
        "formatting withdrawn",
    );
    assert!(
        caps.get("documentRangeFormattingProvider").is_none() || caps["documentRangeFormattingProvider"].is_null(),
        "range formatting withdrawn",
    );
    assert!(
        caps.get("codeActionProvider").is_none() || caps["codeActionProvider"].is_null(),
        "code actions withdrawn",
    );
}

#[tokio::test]
async fn did_open_publishes_diagnostics() {
    let (mut service, socket) = build_service_for_tests();
    let _body = initialize(&mut service, true).await;

    let uri = "file:///tmp/mdwright-test-open.md";
    let source = "See https://example.com for details.\n";
    open_doc(&mut service, uri, 1, source).await;

    let published = wait_for_publish(socket, uri).await;
    let diags = published["diagnostics"].as_array().expect("diagnostics array");
    assert!(
        diags
            .iter()
            .any(|d| d["code"].as_str() == Some("bare-url") && d["source"].as_str() == Some("mdwright")),
        "expected a bare-url diagnostic from {source:?}, got {diags:?}",
    );
}

#[tokio::test]
async fn formatting_returns_expected_textedit() {
    let (mut service, _socket) = build_service_for_tests();
    let _body = initialize(&mut service, true).await;

    let uri = "file:///tmp/mdwright-test-fmt.md";
    // The LSP server discovers the repo's own `.mdwright.toml`
    // (`wrap = 120`), so the source must be one the loaded config
    // demonstrably rewrites. CRLF normalisation to LF is the
    // shortest-path guaranteed edit (`end_of_line = "lf"` default).
    let source = "alpha\r\nbeta\r\n";
    open_doc(&mut service, uri, 1, source).await;

    let body = request(
        &mut service,
        42,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    )
    .await;
    let edits = body.as_array().cloned().unwrap_or_default();
    assert!(
        !edits.is_empty(),
        "format should produce at least one edit for {source:?}"
    );
    let edit = &edits[0];
    let new_text = edit["newText"].as_str().expect("newText is a string");
    // The LSP server discovers the repo's `.mdwright.toml` via the
    // server's CWD; mirror the discovery here so the expected output
    // uses the same fmt options.
    let cfg = mdwright_config::Config::discover(
        std::env::current_dir()
            .as_deref()
            .unwrap_or_else(|_| std::path::Path::new(".")),
    )
    .unwrap_or_else(|_| mdwright_config::Config::defaults());
    let expected = mdwright_format::format_document(
        &mdwright_document::Document::parse(source).expect("fixture parses"),
        cfg.fmt_options(),
    );
    assert_eq!(new_text, expected, "LSP format must match CLI format byte-for-byte");
}

#[tokio::test]
async fn parser_panic_input_publishes_parse_diagnostic_and_recovers() {
    let (mut service, mut socket) = build_service_for_tests();
    let _body = initialize(&mut service, true).await;

    let uri = "file:///tmp/mdwright-test-parser-boundary.md";
    let source = "- [n]:Z\r\n\t\t";
    open_doc(&mut service, uri, 1, source).await;

    let published = wait_for_publish(&mut socket, uri).await;
    let diags = published["diagnostics"].as_array().expect("diagnostics array");
    assert_eq!(diags.len(), 1, "parse failure should suppress lint diagnostics");
    assert!(
        diags[0]["message"]
            .as_str()
            .is_some_and(|msg| msg.contains("Markdown parser failed")),
        "unexpected parse diagnostic: {diags:?}",
    );
    assert_eq!(diags[0]["range"]["start"]["line"], 0);
    assert_eq!(diags[0]["range"]["start"]["character"], 0);
    assert_eq!(diags[0]["range"]["end"]["line"], 0);
    assert_eq!(diags[0]["range"]["end"]["character"], 0);

    let body = request(
        &mut service,
        43,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    )
    .await;
    assert!(body.is_null(), "parse-failed document returns no edits");

    let body = request(
        &mut service,
        44,
        "textDocument/rangeFormatting",
        json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 1 },
            },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    )
    .await;
    assert!(body.is_null(), "parse-failed range format returns no edits");

    let body = request(
        &mut service,
        45,
        "textDocument/onTypeFormatting",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 0 },
            "ch": "\n",
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    )
    .await;
    assert!(body.is_null(), "parse-failed on-type format returns no edits");

    let body = request(
        &mut service,
        46,
        "textDocument/codeAction",
        json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 0 },
            },
            "context": { "diagnostics": [] },
        }),
    )
    .await;
    assert!(body.is_null(), "parse-failed fix-all returns no actions");

    change_doc(&mut service, uri, 2, "See https://example.com now.\n").await;

    let published = wait_for_publish(&mut socket, uri).await;
    let diags = published["diagnostics"].as_array().expect("diagnostics array");
    assert!(
        diags.iter().any(|diag| diag["code"].as_str() == Some("bare-url")),
        "expected normal lint diagnostics after parseable edit, got {diags:?}",
    );
}

#[tokio::test]
async fn range_formatting_uses_verified_formatter_output() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(temp.path().join(".mdwright.toml"), "[fmt]\nwrap = 40\n").expect("write config");
    let root_uri = Url::from_directory_path(temp.path())
        .expect("directory uri")
        .to_string();
    let (mut service, _socket) = build_service_for_tests();
    let _body = initialize_with_root(&mut service, true, Some(&root_uri)).await;

    let uri = Url::from_file_path(temp.path().join("range.md"))
        .expect("file uri")
        .to_string();
    let source =
        "This is a long paragraph that should wrap when the LSP range formatter delegates to mdwright-format.\n";
    open_doc(&mut service, &uri, 1, source).await;

    let body = request(
        &mut service,
        47,
        "textDocument/rangeFormatting",
        json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": source.len() },
            },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    )
    .await;
    let edits = body.as_array().cloned().expect("range formatting returns edits");
    assert_eq!(edits.len(), 1, "expected one snapped range edit: {edits:?}");
    let got = edits[0]["newText"].as_str().expect("newText");

    let cfg = mdwright_config::Config::discover(temp.path()).expect("config");
    let doc = mdwright_document::Document::parse_with_options(source, cfg.parse_options()).expect("fixture parses");
    let checkpoints = mdwright_format::CheckpointTable::from_document(&doc);
    let expected =
        mdwright_format::format_range_with_checkpoints(&doc, cfg.fmt_options(), &checkpoints, 0..source.len());
    assert_eq!(got, expected);
}

#[tokio::test]
async fn stale_change_does_not_overwrite_newer_document_state() {
    let (mut service, _socket) = build_service_for_tests();
    let _body = initialize(&mut service, true).await;

    let uri = "file:///tmp/mdwright-test-stale-version.md";
    let source = "alpha\r\nbeta\r\n";
    open_doc(&mut service, uri, 2, source).await;
    change_doc(&mut service, uri, 1, "- [n]:Z\r\n\t\t").await;

    let body = request(
        &mut service,
        48,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    )
    .await;
    let edits = body.as_array().cloned().unwrap_or_default();
    assert!(
        !edits.is_empty(),
        "stale parse-failing change must not replace the newer parseable text"
    );
}

#[tokio::test]
async fn oversized_document_publishes_diagnostic_and_does_not_format() {
    let (mut service, mut socket) = build_service_for_tests();
    let _body = initialize(&mut service, true).await;

    let uri = "file:///tmp/mdwright-test-large.md";
    let source = "a".repeat(10_000_001);
    open_doc(&mut service, uri, 1, &source).await;

    let published = wait_for_publish(&mut socket, uri).await;
    let diags = published["diagnostics"].as_array().expect("diagnostics array");
    assert_eq!(diags.len(), 1, "oversized input should suppress lint diagnostics");
    assert!(
        diags[0]["message"]
            .as_str()
            .is_some_and(|msg| msg.contains("exceeds the mdwright LSP limit")),
        "unexpected size diagnostic: {diags:?}",
    );

    let body = request(
        &mut service,
        49,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    )
    .await;
    assert!(body.is_null(), "oversized document returns no format edits");
}

#[tokio::test]
async fn root_config_discovery_controls_lsp_diagnostics() {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        temp.path().join(".mdwright.toml"),
        "[lint]\npreset = \"default\"\nignore = [\"bare-url\"]\n",
    )
    .expect("write config");
    let root_uri = Url::from_directory_path(temp.path())
        .expect("directory uri")
        .to_string();
    let (mut service, socket) = build_service_for_tests();
    let _body = initialize_with_root(&mut service, true, Some(&root_uri)).await;

    let uri = Url::from_file_path(temp.path().join("doc.md"))
        .expect("file uri")
        .to_string();
    open_doc(&mut service, &uri, 1, "See https://example.com now.\n").await;

    let published = wait_for_publish(socket, &uri).await;
    let diags = published["diagnostics"].as_array().expect("diagnostics array");
    assert!(
        diags.iter().all(|diag| diag["code"].as_str() != Some("bare-url")),
        "root config should disable bare-url diagnostics: {diags:?}",
    );
}

#[tokio::test]
async fn config_reload_recomputes_parse_options_for_open_documents() {
    let temp = tempfile::tempdir().expect("tempdir");
    let config_path = temp.path().join(".mdwright.toml");
    std::fs::write(
        &config_path,
        "[fmt]\nheading-attrs = \"canonicalise\"\n[parse.extensions]\nheading-attribute-lists = false\n",
    )
    .expect("write config");
    let root_uri = Url::from_directory_path(temp.path())
        .expect("directory uri")
        .to_string();
    let (mut service, mut socket) = build_service_for_tests();
    let _body = initialize_with_root(&mut service, true, Some(&root_uri)).await;

    let uri = Url::from_file_path(temp.path().join("doc.md"))
        .expect("file uri")
        .to_string();
    let source = "# Title {.b #a}\n";
    open_doc(&mut service, &uri, 1, source).await;
    let _initial_publish = wait_for_publish(&mut socket, &uri).await;

    let body = request(
        &mut service,
        50,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    )
    .await;
    assert!(
        body.as_array().is_some_and(Vec::is_empty),
        "disabled heading attrs should leave source unchanged: {body:?}",
    );

    std::fs::write(
        &config_path,
        "[fmt]\nheading-attrs = \"canonicalise\"\n[parse.extensions]\nheading-attribute-lists = true\n",
    )
    .expect("rewrite config");
    let _ack = service
        .ready()
        .await
        .expect("service ready")
        .call(notif(
            "workspace/didChangeWatchedFiles",
            json!({
                "changes": [{
                    "uri": Url::from_file_path(&config_path).expect("config uri").to_string(),
                    "type": 2
                }],
            }),
        ))
        .await
        .expect("call ok");
    let _reload_publish = wait_for_publish(&mut socket, &uri).await;

    let body = request(
        &mut service,
        51,
        "textDocument/formatting",
        json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    )
    .await;
    let edits = body.as_array().cloned().expect("formatting edits after reload");
    assert!(
        edits
            .iter()
            .any(|edit| edit["newText"].as_str() == Some("# Title {#a .b}\n")),
        "config reload should rebuild document facts with updated parse policy: {edits:?}",
    );
}

async fn wait_for_publish<S>(mut socket: S, uri: &str) -> Value
where
    S: futures::Stream<Item = Request> + Unpin,
{
    let timeout = Duration::from_secs(5);
    loop {
        let next = tokio::time::timeout(timeout, socket.next()).await;
        let Ok(Some(msg)) = next else {
            panic!("socket closed or timed out waiting for publishDiagnostics for {uri}");
        };
        if msg.method() == "textDocument/publishDiagnostics" {
            let params = msg.params().cloned().unwrap_or(Value::Null);
            if params.get("uri").and_then(Value::as_str) == Some(uri) {
                return params;
            }
        }
    }
}
