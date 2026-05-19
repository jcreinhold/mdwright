//! Language Server Protocol front-end for the mdwright linter and
//! formatter.
//!
//! The server runs over stdio and exposes:
//!
//! - `textDocument/publishDiagnostics` — debounced lint output (300 ms
//!   after the last `didChange`).
//! - `textDocument/formatting` and `textDocument/rangeFormatting` —
//!   single-pass and block-snapped formatting via the same document and
//!   format crates the CLI uses.
//! - `textDocument/onTypeFormatting` — zero-width range format at the
//!   cursor on each `\n`.
//! - `textDocument/codeAction` — per-diagnostic `quickfix` actions for
//!   every safe fix, plus a bundled
//!   `source.fixAll.mdwright` action.
//! - `textDocument/hover` — long-form `mdwright explain <rule>` text
//!   when the cursor sits inside a diagnostic span.
//!
//! Position encoding is negotiated to UTF-8 in `initialize`; clients
//! that don't grant UTF-8 see diagnostics + hover only (formatting and
//! code actions are withdrawn rather than risk corrupting non-ASCII
//! sources where [`crate::LineIndex::byte_of_position_0based`]'s
//! codepoint counts would mismatch the client's UTF-16 indices).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rustc_hash::FxHashMap;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability,
    CodeActionResponse, CodeDescription, Diagnostic as LspDiagnostic, DiagnosticSeverity, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidChangeWatchedFilesRegistrationOptions,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentFormattingParams,
    DocumentOnTypeFormattingOptions, DocumentOnTypeFormattingParams, DocumentRangeFormattingParams, FileSystemWatcher,
    GlobPattern, Hover, HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    InitializedParams, MarkupContent, MarkupKind, MessageType, NumberOrString, OneOf, Position, PositionEncodingKind,
    Range, Registration, ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit,
    Url, WorkDoneProgressOptions, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

use mdwright_config::Config;
use mdwright_document::{Document, LineIndex, ParseError, ParseOptions};
use mdwright_format::{CheckpointTable, FmtOptions, format_document, format_range_with_checkpoints};
use mdwright_lint::{
    Diagnostic as MdwrightDiagnostic, RuleSet, Severity as MdwrightSeverity, apply_safe_fixes, rule_doc_url, stdlib,
};

const LSP_MAX_INPUT_BYTES: usize = 10_000_000;

/// Run the LSP server over stdio. Returns when the client sends
/// `exit` (or when the transport closes).
pub async fn serve() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(MdwrightLs::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// Public hook for `LspService::new`. Exposed so `tests/lsp.rs` can
/// construct a server in-process and drive it through `tower::Service`.
/// Not part of the documented surface; the only intended caller is
/// the in-tree LSP test suite.
#[doc(hidden)]
#[must_use]
pub fn build_service_for_tests() -> (LspService<impl LanguageServer + 'static>, tower_lsp::ClientSocket) {
    LspService::new(MdwrightLs::new)
}

struct MdwrightLs {
    client: Client,
    state: Arc<Mutex<State>>,
}

struct State {
    docs: FxHashMap<Url, OpenDoc>,
    config: Arc<Config>,
    root: Option<PathBuf>,
    revision: u64,
    /// Whether the client granted UTF-8 position encoding. Formatting
    /// and code-action providers are gated on this; without it,
    /// [`LineIndex::byte_of_position_0based`] would return wrong byte
    /// offsets for any non-ASCII source the client sent in UTF-16
    /// units.
    utf8: bool,
}

struct OpenDoc {
    text: String,
    version: i32,
    line_index: Arc<LineIndex>,
    recognition: RecognitionState,
    diagnostics: Vec<MdwrightDiagnostic>,
    lint_task: Option<JoinHandle<()>>,
}

#[derive(Clone)]
enum RecognitionState {
    Parsed {
        document: Arc<Document>,
        checkpoints: Arc<CheckpointTable>,
    },
    ParseFailed(ParseError),
    TooLarge {
        len: usize,
        cap: usize,
    },
}

impl RecognitionState {
    fn recognize(text: &str, parse_options: ParseOptions) -> Self {
        if text.len() > LSP_MAX_INPUT_BYTES {
            return Self::TooLarge {
                len: text.len(),
                cap: LSP_MAX_INPUT_BYTES,
            };
        }
        match Document::parse_with_options(text, parse_options) {
            Ok(document) => {
                let document = Arc::new(document);
                let checkpoints = Arc::new(CheckpointTable::from_document(&document));
                Self::Parsed { document, checkpoints }
            }
            Err(err) => Self::ParseFailed(err),
        }
    }

    fn document(&self) -> Option<Arc<Document>> {
        match self {
            Self::Parsed { document, .. } => Some(Arc::clone(document)),
            Self::ParseFailed(_) | Self::TooLarge { .. } => None,
        }
    }

    fn parsed(&self) -> Option<(Arc<Document>, Arc<CheckpointTable>)> {
        match self {
            Self::Parsed { document, checkpoints } => Some((Arc::clone(document), Arc::clone(checkpoints))),
            Self::ParseFailed(_) | Self::TooLarge { .. } => None,
        }
    }
}

impl OpenDoc {
    fn new(text: String, version: i32, parse_options: ParseOptions) -> Self {
        let line_index = Arc::new(LineIndex::new(&text));
        let recognition = RecognitionState::recognize(&text, parse_options);
        Self {
            text,
            version,
            line_index,
            recognition,
            diagnostics: Vec::new(),
            lint_task: None,
        }
    }

    fn replace(&mut self, text: String, version: i32, parse_options: ParseOptions) {
        self.line_index = Arc::new(LineIndex::new(&text));
        self.recognition = RecognitionState::recognize(&text, parse_options);
        self.text = text;
        self.version = version;
        if let Some(prev) = self.lint_task.take() {
            prev.abort();
        }
    }

    fn rebuild_recognition(&mut self, parse_options: ParseOptions) {
        self.line_index = Arc::new(LineIndex::new(&self.text));
        self.recognition = RecognitionState::recognize(&self.text, parse_options);
        if let Some(prev) = self.lint_task.take() {
            prev.abort();
        }
    }
}

impl MdwrightLs {
    fn new(client: Client) -> Self {
        let state = State {
            docs: FxHashMap::default(),
            config: Arc::new(Config::defaults()),
            root: None,
            revision: 0,
            utf8: false,
        };
        Self {
            client,
            state: Arc::new(Mutex::new(state)),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for MdwrightLs {
    async fn initialize(&self, params: InitializeParams) -> RpcResult<InitializeResult> {
        let utf8 = client_supports_utf8(&params);
        let root = workspace_root(&params);
        let config = Arc::new(discover_or_default(root.as_deref()));

        {
            let mut state = self.state.lock().await;
            state.utf8 = utf8;
            state.root = root;
            state.config = config;
            state.revision = state.revision.wrapping_add(1);
        }

        let position_encoding = if utf8 { Some(PositionEncodingKind::UTF8) } else { None };
        let format_provider = if utf8 { Some(OneOf::Left(true)) } else { None };
        let range_format_provider = if utf8 { Some(OneOf::Left(true)) } else { None };
        let on_type_format_provider = if utf8 {
            Some(DocumentOnTypeFormattingOptions {
                first_trigger_character: "\n".into(),
                more_trigger_character: None,
            })
        } else {
            None
        };
        let code_action_provider = if utf8 {
            Some(CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX, CodeActionKind::SOURCE_FIX_ALL]),
                resolve_provider: Some(false),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }))
        } else {
            None
        };

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding,
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                document_formatting_provider: format_provider,
                document_range_formatting_provider: range_format_provider,
                document_on_type_formatting_provider: on_type_format_provider,
                code_action_provider,
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "mdwright".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        if !self.state.lock().await.utf8 {
            self.client
                .log_message(
                    MessageType::WARNING,
                    "mdwright LSP: client did not negotiate UTF-8 position encoding; \
                     formatting and code-action providers are disabled to avoid corrupting \
                     non-ASCII sources. Upgrade to a client that supports UTF-8 \
                     (VS Code 1.74+, Helix, Zed, neovim 0.10+) to enable them.",
                )
                .await;
        }

        let watchers = vec![
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/.mdwright.toml".into()),
                kind: None,
            },
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/mdwright.toml".into()),
                kind: None,
            },
            FileSystemWatcher {
                glob_pattern: GlobPattern::String("**/pyproject.toml".into()),
                kind: None,
            },
        ];
        let registration = Registration {
            id: "mdwright-config-watch".into(),
            method: "workspace/didChangeWatchedFiles".into(),
            register_options: serde_json::to_value(DidChangeWatchedFilesRegistrationOptions { watchers }).ok(),
        };
        // Fire-and-forget: `register_capability` awaits a client
        // response that we don't act on, and blocking `initialized`
        // on it would stall every test harness (and any client that
        // declines the registration). Spawn instead.
        let client = self.client.clone();
        tokio::spawn(async move {
            let _ignore = client.register_capability(vec![registration]).await;
        });
    }

    async fn shutdown(&self) -> RpcResult<()> {
        {
            let mut state = self.state.lock().await;
            for doc in state.docs.values_mut() {
                if let Some(task) = doc.lint_task.take() {
                    task.abort();
                }
            }
        }
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        let version = params.text_document.version;
        let mut state = self.state.lock().await;
        let parse_options = state.config.parse_options();
        let doc = OpenDoc::new(text, version, parse_options);
        state.docs.insert(uri.clone(), doc);
        let (text, version, line_index, recognition, rules, revision) = {
            let Some(entry) = state.docs.get(&uri) else { return };
            (
                entry.text.clone(),
                entry.version,
                Arc::clone(&entry.line_index),
                entry.recognition.clone(),
                build_rules(&state.config),
                state.revision,
            )
        };
        drop(state);
        self.lint_and_publish(uri, text, version, line_index, recognition, rules, revision)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let Some(change) = params.content_changes.into_iter().next() else {
            return;
        };
        let text = change.text;
        let mut state = self.state.lock().await;
        let parse_options = state.config.parse_options();
        let entry = state
            .docs
            .entry(uri.clone())
            .or_insert_with(|| OpenDoc::new(String::new(), version, parse_options));
        if version < entry.version {
            tracing::warn!(
                uri = %uri,
                incoming_version = version,
                current_version = entry.version,
                "ignored stale textDocument/didChange"
            );
            return;
        }
        entry.replace(text, version, parse_options);
        let text_snapshot = entry.text.clone();
        let line_index = Arc::clone(&entry.line_index);
        let recognition = entry.recognition.clone();
        let rules = build_rules(&state.config);
        let revision = state.revision;
        let state_handle = Arc::clone(&self.state);
        let client = self.client.clone();
        let uri_for_task = uri.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            run_lint_pass(
                &client,
                &state_handle,
                uri_for_task,
                text_snapshot,
                version,
                line_index,
                recognition,
                rules,
                revision,
            )
            .await;
        });
        if let Some(entry) = state.docs.get_mut(&uri) {
            entry.lint_task = Some(task);
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let state = self.state.lock().await;
        let Some(entry) = state.docs.get(&uri) else {
            return;
        };
        let text = entry.text.clone();
        let version = entry.version;
        let line_index = Arc::clone(&entry.line_index);
        let recognition = entry.recognition.clone();
        let rules = build_rules(&state.config);
        let revision = state.revision;
        drop(state);
        self.lint_and_publish(uri, text, version, line_index, recognition, rules, revision)
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let mut state = self.state.lock().await;
        if let Some(mut doc) = state.docs.remove(&uri)
            && let Some(task) = doc.lint_task.take()
        {
            task.abort();
        }
        drop(state);
        // Clear stale diagnostics from the editor's view.
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }

    async fn did_change_configuration(&self, _: DidChangeConfigurationParams) {
        self.refresh_config_and_relint().await;
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        self.refresh_config_and_relint().await;
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> RpcResult<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let state = self.state.lock().await;
        if !state.utf8 {
            return Ok(None);
        }
        let Some(doc) = state.docs.get(&uri) else {
            return Ok(None);
        };
        let opts = state.config.fmt_options().clone();
        let source = doc.text.clone();
        let line_index = Arc::clone(&doc.line_index);
        let Some(parsed) = doc.recognition.document() else {
            tracing::warn!("formatting skipped because document is not recognised");
            return Ok(None);
        };
        drop(state);
        let formatted = format_document(&parsed, &opts);
        if formatted == source {
            return Ok(Some(Vec::new()));
        }
        let range = whole_doc_range(&line_index, &source);
        Ok(Some(vec![TextEdit {
            range,
            new_text: formatted,
        }]))
    }

    async fn range_formatting(&self, params: DocumentRangeFormattingParams) -> RpcResult<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        let lsp_range = params.range;
        let state = self.state.lock().await;
        if !state.utf8 {
            return Ok(None);
        }
        let Some(doc) = state.docs.get(&uri) else {
            return Ok(None);
        };
        let opts = state.config.fmt_options().clone();
        let source = doc.text.clone();
        let line_index = Arc::clone(&doc.line_index);
        let Some((parsed, table)) = doc.recognition.parsed() else {
            tracing::warn!("range formatting skipped because document is not recognised");
            return Ok(None);
        };
        drop(state);
        Ok(format_range_edits(
            &source,
            &parsed,
            &line_index,
            &table,
            &opts,
            lsp_range,
        ))
    }

    async fn on_type_formatting(&self, params: DocumentOnTypeFormattingParams) -> RpcResult<Option<Vec<TextEdit>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let state = self.state.lock().await;
        if !state.utf8 {
            return Ok(None);
        }
        let Some(doc) = state.docs.get(&uri) else {
            return Ok(None);
        };
        let opts = state.config.fmt_options().clone();
        let source = doc.text.clone();
        let line_index = Arc::clone(&doc.line_index);
        let Some((parsed, table)) = doc.recognition.parsed() else {
            tracing::warn!("on-type formatting skipped because document is not recognised");
            return Ok(None);
        };
        drop(state);
        let zero_width = Range {
            start: position,
            end: position,
        };
        Ok(format_range_edits(
            &source,
            &parsed,
            &line_index,
            &table,
            &opts,
            zero_width,
        ))
    }

    async fn code_action(&self, params: CodeActionParams) -> RpcResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let requested_range = params.range;
        let state = self.state.lock().await;
        if !state.utf8 {
            return Ok(None);
        }
        let Some(doc) = state.docs.get(&uri) else {
            return Ok(None);
        };
        let line_index = Arc::clone(&doc.line_index);
        let source = doc.text.clone();
        let diags = doc.diagnostics.clone();
        let parsed = doc.recognition.document();
        drop(state);

        let mut actions: Vec<CodeActionOrCommand> = Vec::new();
        let req_start = byte_of_position(&line_index, &source, requested_range.start);
        let req_end = byte_of_position(&line_index, &source, requested_range.end);

        for diag in &diags {
            let Some(fix) = diag.fix.as_ref() else { continue };
            if !fix.safe {
                continue;
            }
            if !overlaps(diag.span.start, diag.span.end, req_start, req_end) {
                continue;
            }
            let Some(range) = to_lsp_range(&line_index, &source, &diag.span) else {
                continue;
            };
            let lsp_diag = mdwright_to_lsp(&line_index, &source, diag);
            let edit = WorkspaceEdit {
                changes: Some(
                    std::iter::once((
                        uri.clone(),
                        vec![TextEdit {
                            range,
                            new_text: fix.replacement.clone(),
                        }],
                    ))
                    .collect(),
                ),
                ..WorkspaceEdit::default()
            };
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Fix `{}`: {}", diag.rule, diag.message),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![lsp_diag]),
                edit: Some(edit),
                is_preferred: Some(true),
                ..CodeAction::default()
            }));
        }

        if diags.iter().any(|d| d.fix.as_ref().is_some_and(|f| f.safe)) {
            let Some(parsed) = parsed else {
                return Ok(if actions.is_empty() { None } else { Some(actions) });
            };
            let (new_text, applied) = apply_safe_fixes(&parsed, &diags);
            if applied > 0 && new_text != source {
                let range = whole_doc_range(&line_index, &source);
                let edit = WorkspaceEdit {
                    changes: Some(std::iter::once((uri.clone(), vec![TextEdit { range, new_text }])).collect()),
                    ..WorkspaceEdit::default()
                };
                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Apply all mdwright safe fixes ({applied})"),
                    kind: Some(CodeActionKind::SOURCE_FIX_ALL),
                    edit: Some(edit),
                    ..CodeAction::default()
                }));
            }
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    async fn hover(&self, params: HoverParams) -> RpcResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let state = self.state.lock().await;
        let Some(doc) = state.docs.get(&uri) else {
            return Ok(None);
        };
        let source = doc.text.clone();
        let line_index = Arc::clone(&doc.line_index);
        let diags = doc.diagnostics.clone();
        drop(state);

        let Some(cursor) = byte_of_position(&line_index, &source, position) else {
            return Ok(None);
        };
        let Some(diag) = diags
            .iter()
            .find(|d| d.span.start <= cursor && cursor < d.span.end.max(d.span.start.saturating_add(1)))
        else {
            return Ok(None);
        };
        let Some(rule) = stdlib::by_name(&diag.rule) else {
            return Ok(None);
        };
        let explain = rule.explain().trim();
        let body = if explain.is_empty() {
            rule.description().to_owned()
        } else {
            format!("**{}**\n\n{explain}\n\n<{}>", rule.name(), rule_doc_url(rule.name()))
        };
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: body,
            }),
            range: to_lsp_range(&line_index, &source, &diag.span),
        }))
    }
}

impl MdwrightLs {
    async fn lint_and_publish(
        &self,
        uri: Url,
        text: String,
        version: i32,
        line_index: Arc<LineIndex>,
        recognition: RecognitionState,
        rules: Arc<RuleSet>,
        revision: u64,
    ) {
        run_lint_pass(
            &self.client,
            &self.state,
            uri,
            text,
            version,
            line_index,
            recognition,
            rules,
            revision,
        )
        .await;
    }

    async fn refresh_config_and_relint(&self) {
        let root = self.state.lock().await.root.clone();
        let new_config = Arc::new(discover_or_default(root.as_deref()));
        let parse_options = new_config.parse_options();
        let to_relint: Vec<(Url, String, i32, Arc<LineIndex>, RecognitionState)> = {
            let mut state = self.state.lock().await;
            state.config = Arc::clone(&new_config);
            state.revision = state.revision.wrapping_add(1);
            state
                .docs
                .iter_mut()
                .map(|(uri, doc)| {
                    doc.rebuild_recognition(parse_options);
                    (
                        uri.clone(),
                        doc.text.clone(),
                        doc.version,
                        Arc::clone(&doc.line_index),
                        doc.recognition.clone(),
                    )
                })
                .collect()
        };
        let rules = build_rules(&new_config);
        let revision = self.state.lock().await.revision;
        for (uri, text, version, line_index, recognition) in to_relint {
            self.lint_and_publish(
                uri,
                text,
                version,
                line_index,
                recognition,
                Arc::clone(&rules),
                revision,
            )
            .await;
        }
    }
}

/// Lint `text` using `rules`, cache the result in `state`, and publish
/// LSP diagnostics. Bails silently if the doc's version has advanced
/// past `version` (a newer change is already in flight).
async fn run_lint_pass(
    client: &Client,
    state: &Arc<Mutex<State>>,
    uri: Url,
    text: String,
    version: i32,
    line_index: Arc<LineIndex>,
    recognition: RecognitionState,
    rules: Arc<RuleSet>,
    revision: u64,
) {
    let (diags, lsp_diags) = match recognition {
        RecognitionState::Parsed { document, .. } => {
            let diags = rules.check(&document);
            let lsp_diags: Vec<LspDiagnostic> = diags.iter().map(|d| mdwright_to_lsp(&line_index, &text, d)).collect();
            (diags, lsp_diags)
        }
        RecognitionState::ParseFailed(err) => (Vec::new(), vec![parse_error_lsp_diag(&err)]),
        RecognitionState::TooLarge { len, cap } => (Vec::new(), vec![input_too_large_lsp_diag(len, cap)]),
    };
    {
        let mut state = state.lock().await;
        if state.revision != revision {
            return;
        }
        if let Some(doc) = state.docs.get_mut(&uri) {
            if doc.version != version {
                return;
            }
            doc.diagnostics = diags;
        } else {
            return;
        }
    }
    client.publish_diagnostics(uri, lsp_diags, Some(version)).await;
}

fn parse_error_lsp_diag(err: &ParseError) -> LspDiagnostic {
    LspDiagnostic {
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(0, 0),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("mdwright".to_owned()),
        message: err.to_string(),
        ..LspDiagnostic::default()
    }
}

fn input_too_large_lsp_diag(len: usize, cap: usize) -> LspDiagnostic {
    LspDiagnostic {
        range: Range {
            start: Position::new(0, 0),
            end: Position::new(0, 0),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("mdwright".to_owned()),
        message: format!("Markdown input is {len} bytes; exceeds the mdwright LSP limit of {cap} bytes"),
        ..LspDiagnostic::default()
    }
}

fn client_supports_utf8(params: &InitializeParams) -> bool {
    params
        .capabilities
        .general
        .as_ref()
        .and_then(|g| g.position_encodings.as_ref())
        .is_some_and(|encs| encs.contains(&PositionEncodingKind::UTF8))
}

fn workspace_root(params: &InitializeParams) -> Option<PathBuf> {
    if let Some(folders) = params.workspace_folders.as_ref()
        && let Some(folder) = folders.first()
        && let Ok(path) = folder.uri.to_file_path()
    {
        return Some(path);
    }
    #[allow(deprecated)]
    if let Some(uri) = params.root_uri.as_ref()
        && let Ok(path) = uri.to_file_path()
    {
        return Some(path);
    }
    None
}

fn discover_or_default(root: Option<&std::path::Path>) -> Config {
    let cwd = root
        .map(std::path::Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    Config::discover(&cwd).unwrap_or_else(|_| Config::defaults())
}

fn build_rules(cfg: &Config) -> Arc<RuleSet> {
    let mut rules = parse_rules_spec(cfg.rules_spec()).unwrap_or_else(|_| RuleSet::stdlib_defaults());
    if !cfg.extra_info_strings().is_empty() && rules.contains("info-string-typo") {
        rules.remove("info-string-typo");
        if let Err(err) = rules.add(Box::new(stdlib::InfoStringTypo::with_extra(
            cfg.extra_info_strings().to_vec(),
        ))) {
            tracing::warn!(error = %err, "failed to re-register info-string-typo with config extras");
        }
    }
    Arc::new(rules)
}

/// Mirror of the binary's `parse_rules_spec`. Kept private here so the
/// LSP server doesn't depend on the CLI binary; the two parsers must
/// stay in sync, fenced by `tests/lsp.rs::cli_and_lsp_parse_rules_alike`.
fn parse_rules_spec(spec: &str) -> Result<RuleSet, String> {
    let tokens: Vec<&str> = spec.split(',').map(str::trim).filter(|t| !t.is_empty()).collect();
    if tokens.is_empty() {
        return Ok(RuleSet::stdlib_defaults());
    }
    let mut rs: Option<RuleSet> = None;
    for tok in tokens {
        if let Some(name) = tok.strip_prefix('+') {
            let rule = stdlib::by_name(name).ok_or_else(|| format!("unknown rule: {name}"))?;
            let target = rs.get_or_insert_with(RuleSet::new);
            if !target.contains(name) {
                target.add(rule).map_err(|e| e.to_string())?;
            }
        } else if let Some(name) = tok.strip_prefix('-') {
            let target = rs.get_or_insert_with(RuleSet::stdlib_defaults);
            target.remove(name);
        } else if tok == "all" {
            rs = Some(RuleSet::stdlib_all());
        } else if tok == "default" {
            rs = Some(RuleSet::stdlib_defaults());
        } else {
            let rule = stdlib::by_name(tok).ok_or_else(|| format!("unknown rule: {tok}"))?;
            let target = rs.get_or_insert_with(RuleSet::new);
            if !target.contains(tok) {
                target.add(rule).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(rs.unwrap_or_else(RuleSet::stdlib_defaults))
}

fn mdwright_to_lsp(line_index: &LineIndex, source: &str, diag: &MdwrightDiagnostic) -> LspDiagnostic {
    let range = to_lsp_range(line_index, source, &diag.span).unwrap_or(Range {
        start: Position { line: 0, character: 0 },
        end: Position { line: 0, character: 0 },
    });
    LspDiagnostic {
        range,
        severity: Some(match diag.severity() {
            MdwrightSeverity::Error => DiagnosticSeverity::WARNING,
            MdwrightSeverity::Warning => DiagnosticSeverity::WARNING,
            MdwrightSeverity::Advisory => DiagnosticSeverity::HINT,
        }),
        code: Some(NumberOrString::String(diag.rule.to_string())),
        code_description: Url::parse(&rule_doc_url(&diag.rule))
            .ok()
            .map(|href| CodeDescription { href }),
        source: Some("mdwright".into()),
        message: diag.message.clone(),
        ..LspDiagnostic::default()
    }
}

fn to_lsp_range(line_index: &LineIndex, source: &str, span: &std::ops::Range<usize>) -> Option<Range> {
    let (sl, sc) = line_index.locate(source, span.start).ok()?;
    let (el, ec) = line_index.locate(source, span.end).ok()?;
    Some(Range {
        start: Position {
            line: u32::try_from(sl.saturating_sub(1)).ok()?,
            character: u32::try_from(sc.saturating_sub(1)).ok()?,
        },
        end: Position {
            line: u32::try_from(el.saturating_sub(1)).ok()?,
            character: u32::try_from(ec.saturating_sub(1)).ok()?,
        },
    })
}

fn whole_doc_range(line_index: &LineIndex, source: &str) -> Range {
    let end = match line_index.locate(source, source.len()) {
        Ok((l, c)) => Position {
            line: u32::try_from(l.saturating_sub(1)).unwrap_or(0),
            character: u32::try_from(c.saturating_sub(1)).unwrap_or(0),
        },
        Err(_) => Position { line: 0, character: 0 },
    };
    Range {
        start: Position { line: 0, character: 0 },
        end,
    }
}

fn byte_of_position(line_index: &LineIndex, source: &str, position: Position) -> Option<usize> {
    line_index.byte_of_position_0based(source, position.line as usize, position.character as usize)
}

fn overlaps(a_start: usize, a_end: usize, b_start: Option<usize>, b_end: Option<usize>) -> bool {
    let (Some(bs), Some(be)) = (b_start, b_end) else {
        return true;
    };
    a_start < be.max(bs.saturating_add(1)) && bs < a_end.max(a_start.saturating_add(1))
}

fn format_range_edits(
    source: &str,
    doc: &Document,
    line_index: &LineIndex,
    table: &CheckpointTable,
    opts: &FmtOptions,
    lsp_range: Range,
) -> Option<Vec<TextEdit>> {
    let lo = byte_of_position(line_index, source, lsp_range.start)?;
    let hi = byte_of_position(line_index, source, lsp_range.end).unwrap_or(source.len());
    let (lo, hi) = if hi < lo { (hi, lo) } else { (lo, hi) };
    let formatted = format_range_with_checkpoints(doc, opts, table, lo..hi);
    // The formatter snaps outward to whole-block boundaries; we need
    // the snapped byte range to compute the LSP edit range. Recompute
    // by asking the checkpoint table.
    let req_lo = u32::try_from(lo).unwrap_or(0);
    let req_hi = u32::try_from(hi).unwrap_or(u32::MAX);
    let snapped = table.snap_to_block_boundaries(req_lo..req_hi);
    let slice_lo = snapped.start as usize;
    let slice_hi = snapped.end as usize;
    if slice_lo >= source.len() && formatted.is_empty() {
        return Some(Vec::new());
    }
    let current = source.get(slice_lo..slice_hi).unwrap_or("");
    if current == formatted {
        return Some(Vec::new());
    }
    let edit_range = to_lsp_range(line_index, source, &(slice_lo..slice_hi))?;
    Some(vec![TextEdit {
        range: edit_range,
        new_text: formatted,
    }])
}

#[cfg(test)]
mod tests {
    use super::{LineIndex, Position, overlaps, parse_rules_spec, whole_doc_range};

    #[test]
    fn parse_rules_spec_default() -> Result<(), String> {
        let rs = parse_rules_spec("default")?;
        assert!(!rs.is_empty());
        Ok(())
    }

    #[test]
    fn parse_rules_spec_subtract() -> Result<(), String> {
        let rs = parse_rules_spec("default,-bare-url")?;
        assert!(!rs.contains("bare-url"));
        Ok(())
    }

    #[test]
    fn parse_rules_spec_bare_name_starts_empty() -> Result<(), String> {
        let rs = parse_rules_spec("bare-url")?;
        assert!(rs.contains("bare-url"));
        assert_eq!(rs.len(), 1);
        Ok(())
    }

    #[test]
    fn overlap_half_open_ranges() {
        // Selection entirely inside the diagnostic span.
        assert!(overlaps(5, 10, Some(7), Some(8)));
        // Empty selection (cursor) at the diagnostic's left edge.
        assert!(overlaps(5, 10, Some(5), Some(5)));
        // Selection strictly to the left or right of the span: no
        // overlap (half-open ranges, touching endpoints don't count).
        assert!(!overlaps(5, 10, Some(0), Some(5)));
        assert!(!overlaps(5, 10, Some(0), Some(4)));
        assert!(!overlaps(5, 10, Some(11), Some(20)));
        // Missing endpoints (degenerate codeAction params) default to
        // "match everything" so we never hide a fix the user might
        // want.
        assert!(overlaps(5, 10, None, None));
    }

    #[test]
    fn whole_doc_range_empty_source() {
        let src = "";
        let idx = LineIndex::new(src);
        let r = whole_doc_range(&idx, src);
        assert_eq!(r.start, Position { line: 0, character: 0 });
        assert_eq!(r.end, Position { line: 0, character: 0 });
    }
}
