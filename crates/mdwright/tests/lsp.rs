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

use futures::StreamExt;
use mdwright_lsp::build_service_for_tests;
use serde_json::{Value, json};
use tower::{Service, ServiceExt};
use tower_lsp::jsonrpc::Request;

fn req(id: i64, method: &'static str, params: Value) -> Request {
    Request::build(method).id(id).params(params).finish()
}

fn notif(method: &'static str, params: Value) -> Request {
    Request::build(method).params(params).finish()
}

fn init_params(utf8: bool) -> Value {
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
        "rootUri": null,
    })
}

async fn initialize(
    service: &mut tower_lsp::LspService<impl tower_lsp::LanguageServer + 'static>,
    utf8: bool,
) -> Value {
    let resp = service
        .ready()
        .await
        .expect("service ready")
        .call(req(1, "initialize", init_params(utf8)))
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
                    "version": 1,
                    "text": source,
                }
            }),
        ))
        .await
        .expect("call ok");

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
                    "version": 1,
                    "text": source,
                }
            }),
        ))
        .await
        .expect("call ok");

    let resp = service
        .ready()
        .await
        .expect("service ready")
        .call(req(
            42,
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": uri },
                "options": { "tabSize": 4, "insertSpaces": true },
            }),
        ))
        .await
        .expect("call ok")
        .expect("formatting returns a response");
    let (_, body) = resp.into_parts();
    let edits = body.expect("formatting Ok").as_array().cloned().unwrap_or_default();
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
    let expected = mdwright_format::format_document(&mdwright_document::Document::parse(source), cfg.fmt_options());
    assert_eq!(new_text, expected, "LSP format must match CLI format byte-for-byte");
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
