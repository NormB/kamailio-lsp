//! The tower-lsp language server.

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::{analyze, catalog, diag, logic};

/// LSP backend: document store, doc catalog, and the `-c` runner.
pub struct Backend {
    client: Client,
    /// Open documents: (version, full text).
    docs: std::sync::Arc<DashMap<Url, (i32, String)>>,
    catalog: std::sync::Arc<std::sync::RwLock<Vec<catalog::ModuleDoc>>>,
    core: std::sync::RwLock<catalog::CoreDocs>,
    src: std::sync::RwLock<Option<String>>,
    wiki: std::sync::RwLock<Option<String>>,
    modules_path: std::sync::RwLock<Option<String>>,
    kamailio_bin: std::sync::RwLock<Option<String>>,
    /// Serializes `kamailio -c` runs: one at a time, no process storm.
    check_gate: std::sync::Arc<tokio::sync::Mutex<()>>,
    /// In-flight check task per document: a newer check for the same
    /// URI aborts the old one (latest wins; `kill_on_drop` reaps the
    /// superseded child process).
    check_tasks: DashMap<Url, tokio::task::JoinHandle<()>>,
    snippet_completions: std::sync::RwLock<bool>,
    max_diagnostics: std::sync::RwLock<usize>,
    cache_dir_opt: std::sync::RwLock<Option<String>>,
    check_timeout: std::sync::RwLock<std::time::Duration>,
    /// Last published `-c` results per document; merged with analyzer
    /// diagnostics on every publish.
    check_diags: std::sync::Arc<DashMap<Url, Vec<Diagnostic>>>,
    /// Fast analyzer diagnostics between saves (init option).
    analyzer_enabled: std::sync::RwLock<bool>,
    /// didChange generation per document: only the latest debounced
    /// analyzer task publishes.
    change_gen: std::sync::Arc<DashMap<Url, u64>>,
    /// Reference-count code lenses on route definitions (init option).
    code_lens_refs: std::sync::RwLock<bool>,
    /// Did the client advertise window.workDoneProgress support?
    work_done_progress: std::sync::RwLock<bool>,
}

impl Backend {
    /// Build a backend for one client connection.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: std::sync::Arc::new(DashMap::new()),
            catalog: std::sync::Arc::new(std::sync::RwLock::new(Vec::new())),
            core: std::sync::RwLock::new(catalog::CoreDocs::default()),
            src: std::sync::RwLock::new(None),
            wiki: std::sync::RwLock::new(None),
            modules_path: std::sync::RwLock::new(None),
            kamailio_bin: std::sync::RwLock::new(None),
            check_gate: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            check_tasks: DashMap::new(),
            snippet_completions: std::sync::RwLock::new(true),
            max_diagnostics: std::sync::RwLock::new(100),
            cache_dir_opt: std::sync::RwLock::new(None),
            check_timeout: std::sync::RwLock::new(logic::resolve_timeout(
                None,
                std::env::var("KAMAILIO_LSP_CHECK_TIMEOUT_MS").ok(),
            )),
            check_diags: std::sync::Arc::new(DashMap::new()),
            analyzer_enabled: std::sync::RwLock::new(true),
            change_gen: std::sync::Arc::new(DashMap::new()),
            code_lens_refs: std::sync::RwLock::new(true),
            work_done_progress: std::sync::RwLock::new(false),
        }
    }

    /// Snapshot of open file-scheme buffers, for include resolution
    /// that prefers editor contents over disk.
    fn open_docs_snapshot(&self) -> std::collections::HashMap<std::path::PathBuf, String> {
        Self::open_docs_snapshot_of(&self.docs)
    }

    /// [`Self::open_docs_snapshot`] over a shared document map (for
    /// spawned check tasks that no longer hold `&self`).
    fn open_docs_snapshot_of(
        docs: &DashMap<Url, (i32, String)>,
    ) -> std::collections::HashMap<std::path::PathBuf, String> {
        docs.iter()
            .filter_map(|e| {
                let url = e.key();
                if url.scheme() != "file" {
                    return None;
                }
                url.to_file_path().ok().map(|p| (p, e.value().1.clone()))
            })
            .collect()
    }

    /// A file loader over an open-buffer snapshot with a disk
    /// fallback, size-capped so a hostile include stays cheap.
    fn make_loader(
        open: std::collections::HashMap<std::path::PathBuf, String>,
    ) -> impl Fn(&std::path::Path) -> Option<String> {
        move |p: &std::path::Path| {
            if let Some(t) = open.get(p) {
                return Some(t.clone());
            }
            let md = std::fs::metadata(p).ok()?;
            if !md.is_file() || md.len() > 1_048_576 {
                return None;
            }
            std::fs::read_to_string(p).ok()
        }
    }

    /// Analyzer diagnostics for `text`, mapped to LSP (UTF-16) ranges.
    fn analyzer_lsp_diags(
        path: &std::path::Path,
        text: &str,
        loader: &dyn Fn(&std::path::Path) -> Option<String>,
        cat: &[catalog::ModuleDoc],
    ) -> Vec<Diagnostic> {
        let mut all = logic::analyzer_diagnostics(path, text, loader);
        all.extend(logic::catalog_diagnostics(cat, text));
        all.into_iter()
            .map(|d| {
                let lt = Self::doc_line(text, d.line);
                Diagnostic {
                    range: Range {
                        start: Position::new(
                            d.line,
                            analyze::byte_to_utf16(&lt, d.col_start as usize),
                        ),
                        end: Position::new(d.line, analyze::byte_to_utf16(&lt, d.col_end as usize)),
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("kamailio-lsp".into()),
                    message: d.message,
                    ..Default::default()
                }
            })
            .collect()
    }

    /// Merge stored `-c` results with fresh analyzer diagnostics and
    /// publish, capped.  Static so the debounce task can call it.
    #[allow(clippy::too_many_arguments)]
    async fn merge_and_publish(
        client: &Client,
        check_map: &DashMap<Url, Vec<Diagnostic>>,
        analyzer_enabled: bool,
        cap: usize,
        uri: &Url,
        version: i32,
        text: &str,
        open: std::collections::HashMap<std::path::PathBuf, String>,
        cat: &std::sync::Arc<std::sync::RwLock<Vec<catalog::ModuleDoc>>>,
        skip_if_empty: bool,
    ) {
        let mut merged = check_map.get(uri).map(|v| v.clone()).unwrap_or_default();
        if analyzer_enabled && let Ok(path) = uri.to_file_path() {
            let loader = Self::make_loader(open);
            let cat = cat.read().unwrap().clone();
            merged.extend(Self::analyzer_lsp_diags(&path, text, &loader, &cat));
        }
        if merged.is_empty() && skip_if_empty {
            return;
        }
        merged.truncate(cap.max(1));
        client
            .publish_diagnostics(uri.clone(), merged, Some(version))
            .await;
    }

    /// The include closure rooted at an open document: the document
    /// itself plus transitively included files (open buffers first,
    /// disk fallback).  Non-file documents get a single-entry closure.
    fn closure_for(&self, uri: &Url, text: &str) -> Vec<(std::path::PathBuf, String)> {
        let Ok(path) = uri.to_file_path() else {
            return vec![(std::path::PathBuf::new(), text.to_string())];
        };
        let loader = Self::make_loader(self.open_docs_snapshot());
        logic::include_closure(&path, text, &loader)
    }

    fn doc_line(text: &str, line: u32) -> String {
        text.lines().nth(line as usize).unwrap_or("").to_string()
    }

    /// The route-name symbol (and namespace) under an LSP (UTF-16)
    /// position, if any.
    fn route_symbol_at(text: &str, pos: Position) -> Option<(String, logic::RouteNs)> {
        let line = text.lines().nth(pos.line as usize)?;
        let byte_col = analyze::utf16_to_byte(line, pos.character) as u32;
        logic::route_symbol_ns_at(text, pos.line, byte_col)
    }

    /// The URI of one closure entry: the root keeps the request's
    /// URI; includes map through their path.
    fn closure_uri(
        root_uri: &Url,
        root_path: &std::path::Path,
        p: &std::path::Path,
    ) -> Option<Url> {
        if p == root_path {
            Some(root_uri.clone())
        } else {
            Url::from_file_path(p).ok()
        }
    }

    /// UTF-16 range of one route-name occurrence.
    fn occurrence_range(text: &str, l: &analyze::Located) -> Range {
        let lt = Self::doc_line(text, l.line);
        let s = analyze::byte_to_utf16(&lt, l.col as usize);
        let e = analyze::byte_to_utf16(&lt, l.col as usize + l.name.len());
        Range {
            start: Position::new(l.line, s),
            end: Position::new(l.line, e),
        }
    }

    fn line_prefix(text: &str, pos: Position) -> String {
        text.lines()
            .nth(pos.line as usize)
            .map(|l| {
                let e = analyze::utf16_to_byte(l, pos.character);
                l[..e].to_string()
            })
            .unwrap_or_default()
    }

    /// Launch (or relaunch) the `-c` check for one document.  Any
    /// in-flight check for the same URI is aborted first: the newest
    /// snapshot always wins, and `kill_on_drop` reaps a superseded
    /// child process.
    fn spawn_check(&self, uri: &Url) {
        if uri.scheme() != "file" {
            return;
        }
        let ctx = CheckCtx {
            client: self.client.clone(),
            docs: self.docs.clone(),
            catalog: self.catalog.clone(),
            check_diags: self.check_diags.clone(),
            check_gate: self.check_gate.clone(),
            bin: self.kamailio_bin.read().unwrap().clone(),
            modules_path: self.modules_path.read().unwrap().clone(),
            check_timeout: *self.check_timeout.read().unwrap(),
            analyzer_enabled: *self.analyzer_enabled.read().unwrap(),
            cap: *self.max_diagnostics.read().unwrap(),
            uri: uri.clone(),
        };
        if let Some((_, old)) = self.check_tasks.remove(uri) {
            old.abort();
        }
        self.check_tasks
            .insert(uri.clone(), tokio::spawn(Self::run_check(ctx)));
    }

    /// The body of one check task; state was snapshotted at spawn.
    async fn run_check(ctx: CheckCtx) {
        let uri = &ctx.uri;
        let Ok(path) = uri.to_file_path() else {
            return;
        };
        let path_str = path.display().to_string();
        // snapshot the buffer BEFORE the subprocess runs: ranges are
        // mapped through exactly this text, and the publish carries
        // exactly this version
        let (snap_version, snap_text) = ctx
            .docs
            .get(uri)
            .map(|d| d.clone())
            .unwrap_or((0, String::new()));
        let analyzer_enabled = ctx.analyzer_enabled;
        let cap = ctx.cap;
        let Some(bin) = ctx.bin.clone() else {
            // -c disabled: analyzer-only pass.  skip_if_empty keeps the
            // no-checks contract quiet for clean documents.
            ctx.check_diags.insert(uri.clone(), Vec::new());
            Self::merge_and_publish(
                &ctx.client,
                &ctx.check_diags,
                analyzer_enabled,
                cap,
                uri,
                snap_version,
                &snap_text,
                Self::open_docs_snapshot_of(&ctx.docs),
                &ctx.catalog,
                true,
            )
            .await;
            return;
        };
        // one -c at a time; a burst of didOpen events must not fork a
        // process per file
        let _gate = ctx.check_gate.lock().await;
        // test-only hook: slow the check down to make races observable
        if let Some(ms) = std::env::var("KAMAILIO_LSP_TEST_CHECK_DELAY_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
        {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        }
        // stream stdout+stderr with a byte cap: the timeout bounds
        // seconds, this bounds memory — a flooding checker is killed
        let out_cap = std::env::var("KAMAILIO_LSP_OUTPUT_CAP_BYTES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1_048_576);
        let modules_path = ctx.modules_path.clone();
        let fut = async {
            // -Y: kamailio needs a writable runtime dir even for -c;
            // --all-errors: report every detectable error in one run
            let mut cmd = tokio::process::Command::new(&bin);
            // cwd parity with the CLI checker: relative include_file
            // paths resolve from the config's own directory
            cmd.current_dir(path.parent().unwrap_or(std::path::Path::new(".")));
            cmd.arg("-c").arg("--all-errors");
            cmd.arg("-Y").arg(std::env::temp_dir());
            if let Some(mp) = &modules_path {
                cmd.arg("-L").arg(mp);
            }
            let mut child = cmd
                .arg("-f")
                .arg(&path_str)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;
            use tokio::io::AsyncReadExt;
            async fn read_capped(
                mut s: impl tokio::io::AsyncRead + Unpin,
                cap: usize,
            ) -> std::io::Result<(Vec<u8>, bool)> {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    let n = s.read(&mut chunk).await?;
                    if n == 0 {
                        return Ok((buf, false));
                    }
                    if buf.len() + n > cap {
                        return Ok((buf, true));
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
            }
            let stdout = child.stdout.take().expect("piped");
            let stderr = child.stderr.take().expect("piped");
            let (o, e) = tokio::join!(read_capped(stdout, out_cap), read_capped(stderr, out_cap));
            let (o_buf, o_capped) = o?;
            let (e_buf, e_capped) = e?;
            if o_capped || e_capped {
                let _ = child.kill().await;
                return Ok::<_, std::io::Error>((None, Vec::new(), Vec::new()));
            }
            let status = child.wait().await?;
            Ok((Some(status), o_buf, e_buf))
        };
        let check_timeout = ctx.check_timeout;
        let out = match tokio::time::timeout(check_timeout, fut).await {
            Ok(r) => r,
            Err(_) => {
                ctx.client
                    .log_message(
                        MessageType::WARNING,
                        format!(
                            "kamailio-lsp: '{bin} -c' timed out after {:?} on {path_str}",
                            check_timeout
                        ),
                    )
                    .await;
                // an incomplete check must not leave stale results pinned
                ctx.check_diags.insert(uri.clone(), Vec::new());
                Self::merge_and_publish(
                    &ctx.client,
                    &ctx.check_diags,
                    analyzer_enabled,
                    cap,
                    uri,
                    snap_version,
                    &snap_text,
                    Self::open_docs_snapshot_of(&ctx.docs),
                    &ctx.catalog,
                    false,
                )
                .await;
                return;
            }
        };
        let Ok((status, o_buf, e_buf)) = out else {
            ctx.client
                .log_message(
                    MessageType::WARNING,
                    format!("kamailio-lsp: cannot run '{bin} -c' (configure kamailioPath)"),
                )
                .await;
            ctx.check_diags.insert(uri.clone(), Vec::new());
            Self::merge_and_publish(
                &ctx.client,
                &ctx.check_diags,
                analyzer_enabled,
                cap,
                uri,
                snap_version,
                &snap_text,
                Self::open_docs_snapshot_of(&ctx.docs),
                &ctx.catalog,
                false,
            )
            .await;
            return;
        };
        let Some(status) = status else {
            // capped: the run's output is untrustworthy — clear and log
            ctx.client
                .log_message(
                    MessageType::WARNING,
                    format!(
                        "kamailio-lsp: '{bin} -c' exceeded the output cap ({out_cap} bytes) on {path_str}; run discarded"
                    ),
                )
                .await;
            ctx.check_diags.insert(uri.clone(), Vec::new());
            Self::merge_and_publish(
                &ctx.client,
                &ctx.check_diags,
                analyzer_enabled,
                cap,
                uri,
                snap_version,
                &snap_text,
                Self::open_docs_snapshot_of(&ctx.docs),
                &ctx.catalog,
                false,
            )
            .await;
            return;
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&o_buf),
            String::from_utf8_lossy(&e_buf)
        );
        let rc = status.code().unwrap_or(-1);
        // the buffer moved while the check ran: results belong to a
        // text that no longer exists — suppress; the next save re-checks
        let current = ctx.docs.get(uri).map(|d| d.0);
        if current.is_some() && current != Some(snap_version) {
            ctx.client
                .log_message(
                    MessageType::INFO,
                    format!("kamailio-lsp: discarding stale check of {path_str} (buffer changed)"),
                )
                .await;
            return;
        }
        // kamailio reports byte columns; the client expects UTF-16 units
        let doc_text = snap_text.clone();
        let parsed = diag::parse_check_output(&text, rc);
        let mut mapped: Vec<diag::Diag> = parsed
            .iter()
            .filter_map(|d| logic::remap_include_diag(&path, &doc_text, d))
            .collect();
        if mapped.is_empty() && rc != 0 {
            // every positioned error pointed elsewhere (e.g. a nested
            // include): a failed check must never render as clean
            let context = parsed
                .first()
                .map(|d| format!("{}, line {}: {}", d.file, d.line + 1, d.message))
                .unwrap_or_else(|| format!("kamailio -c failed (rc={rc})"));
            mapped.push(diag::Diag {
                file: path_str.clone(),
                line: 0,
                end_line: 0,
                col_start: 0,
                col_end: 1,
                severity: diag::Severity::Error,
                message: format!("check failed in {context}"),
            });
        }
        let diags: Vec<Diagnostic> = mapped
            .into_iter()
            .map(|d| Diagnostic {
                range: {
                    let start_lt = Self::doc_line(&doc_text, d.line);
                    let end_lt = if d.end_line == d.line {
                        start_lt.clone()
                    } else {
                        Self::doc_line(&doc_text, d.end_line)
                    };
                    Range {
                        start: Position::new(
                            d.line,
                            analyze::byte_to_utf16(&start_lt, d.col_start as usize),
                        ),
                        end: Position::new(
                            d.end_line,
                            analyze::byte_to_utf16(&end_lt, d.col_end as usize),
                        ),
                    }
                },
                severity: Some(match d.severity {
                    diag::Severity::Error => DiagnosticSeverity::ERROR,
                    diag::Severity::Warning => DiagnosticSeverity::WARNING,
                }),
                source: Some("kamailio -c".into()),
                message: d.message,
                ..Default::default()
            })
            .collect();
        let mut diags = diags;
        if diags.len() > cap {
            ctx.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "kamailio-lsp: {} diagnostics on {path_str}, publishing the first {cap} (maxDiagnostics)",
                        diags.len()
                    ),
                )
                .await;
            diags.truncate(cap);
        }
        ctx.check_diags.insert(uri.clone(), diags);
        Self::merge_and_publish(
            &ctx.client,
            &ctx.check_diags,
            analyzer_enabled,
            cap,
            uri,
            snap_version,
            &snap_text,
            Self::open_docs_snapshot_of(&ctx.docs),
            &ctx.catalog,
            false,
        )
        .await;
    }
}

/// Everything one spawned check task needs, snapshotted at spawn
/// time so the task owns its state outright.
struct CheckCtx {
    client: Client,
    docs: std::sync::Arc<DashMap<Url, (i32, String)>>,
    catalog: std::sync::Arc<std::sync::RwLock<Vec<catalog::ModuleDoc>>>,
    check_diags: std::sync::Arc<DashMap<Url, Vec<Diagnostic>>>,
    check_gate: std::sync::Arc<tokio::sync::Mutex<()>>,
    bin: Option<String>,
    modules_path: Option<String>,
    check_timeout: std::time::Duration,
    analyzer_enabled: bool,
    cap: usize,
    uri: Url,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, p: InitializeParams) -> Result<InitializeResult> {
        // progress is only sent to clients that can render it
        *self.work_done_progress.write().unwrap() = p
            .capabilities
            .window
            .as_ref()
            .and_then(|w| w.work_done_progress)
            .unwrap_or(false);
        // Resolution order: initializationOptions, then environment.
        let opts = p.initialization_options.unwrap_or_default();
        let bin = logic::resolve_bin(
            opts.get("kamailioPath").and_then(|v| v.as_str()),
            std::env::var("KAMAILIO_LSP_BIN").ok(),
        );
        *self.kamailio_bin.write().unwrap() = bin;

        if let Some(b) = opts.get("snippetCompletions").and_then(|v| v.as_bool()) {
            *self.snippet_completions.write().unwrap() = b;
        }
        if let Some(b) = opts.get("analyzerDiagnostics").and_then(|v| v.as_bool()) {
            *self.analyzer_enabled.write().unwrap() = b;
        }
        if let Some(b) = opts.get("codeLensReferences").and_then(|v| v.as_bool()) {
            *self.code_lens_refs.write().unwrap() = b;
        }
        if let Some(n) = opts.get("maxDiagnostics").and_then(|v| v.as_u64()) {
            *self.max_diagnostics.write().unwrap() = (n as usize).max(1);
        }
        if let Some(d) = opts.get("cacheDir").and_then(|v| v.as_str())
            && !d.is_empty()
        {
            *self.cache_dir_opt.write().unwrap() = Some(d.to_string());
        }
        if let Some(ms) = opts.get("checkTimeoutMs").and_then(|v| v.as_u64()) {
            *self.check_timeout.write().unwrap() = logic::resolve_timeout(
                Some(ms),
                std::env::var("KAMAILIO_LSP_CHECK_TIMEOUT_MS").ok(),
            );
        }

        let src = opts
            .get("kamailioSrc")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("KAMAILIO_LSP_SRC").ok())
            .filter(|s| !s.is_empty());
        // the harvest happens in `initialized` so a large tree never
        // delays the initialize handshake
        *self.src.write().unwrap() = src;
        let wiki = opts
            .get("kamailioWiki")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("KAMAILIO_LSP_WIKI").ok())
            .filter(|s| !s.is_empty());
        *self.wiki.write().unwrap() = wiki;
        if let Some(mp) = opts.get("modulesPath").and_then(|v| v.as_str())
            && !mp.is_empty()
        {
            *self.modules_path.write().unwrap() = Some(mp.to_string());
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "kamailio-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["\"".into(), "$".into()]),
                    ..Default::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::FUNCTION,
                                    SemanticTokenType::VARIABLE,
                                ],
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: Some(true),
                            work_done_progress_options: Default::default(),
                        },
                    ),
                ),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let src = self.src.read().unwrap().clone();
        let wiki = self.wiki.read().unwrap().clone();
        let mut cached = false;
        if let Some(src) = src {
            // progress reporting: only for clients that advertised
            // window.workDoneProgress, and only if create succeeds
            let token = NumberOrString::String("kamailio-lsp/harvest".into());
            let progress_active = *self.work_done_progress.read().unwrap()
                && self
                    .client
                    .send_request::<request::WorkDoneProgressCreate>(WorkDoneProgressCreateParams {
                        token: token.clone(),
                    })
                    .await
                    .is_ok();
            if progress_active {
                self.client
                    .send_notification::<notification::Progress>(ProgressParams {
                        token: token.clone(),
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                            WorkDoneProgressBegin {
                                title: "Harvesting Kamailio documentation".into(),
                                cancellable: Some(false),
                                message: Some(format!("scanning {src}")),
                                percentage: None,
                            },
                        )),
                    })
                    .await;
                self.client
                    .send_notification::<notification::Progress>(ProgressParams {
                        token: token.clone(),
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::Report(
                            WorkDoneProgressReport {
                                cancellable: Some(false),
                                message: Some("module READMEs and core cookbooks".into()),
                                percentage: None,
                            },
                        )),
                    })
                    .await;
            }
            // the harvest runs off the executor thread and outside the
            // handshake; results are cached per (tree, wiki) fingerprint
            let cache_opt = self.cache_dir_opt.read().unwrap().clone();
            let src_for_task = src.clone();
            let wiki_for_task = wiki.clone();
            let (harvested, core, hit) = tokio::task::spawn_blocking(move || {
                let src = src_for_task;
                let wiki = wiki_for_task;
                let p = std::path::Path::new(&src);
                let wiki_path = wiki.as_deref().map(std::path::Path::new);
                let cache_dir = cache_opt
                    .map(std::path::PathBuf::from)
                    .or_else(|| {
                        std::env::var("KAMAILIO_LSP_CACHE_DIR")
                            .map(std::path::PathBuf::from)
                            .ok()
                    })
                    .or_else(|| {
                        std::env::var("XDG_CACHE_HOME")
                            .map(std::path::PathBuf::from)
                            .ok()
                            .or_else(|| {
                                std::env::var("HOME")
                                    .map(|h| std::path::PathBuf::from(h).join(".cache"))
                                    .ok()
                            })
                            .map(|c| c.join("kamailio-lsp"))
                    });
                if let Some(dir) = &cache_dir
                    && let Some((m, c)) = catalog::load_cached(p, wiki_path, dir)
                {
                    return (m, c, true);
                }
                let core = wiki_path.map(catalog::harvest_core).unwrap_or_default();
                let out = (catalog::harvest_tree(p), core);
                if let Some(dir) = &cache_dir {
                    let _ = catalog::save_cache(p, wiki_path, dir, &out.0, &out.1);
                }
                (out.0, out.1, false)
            })
            .await
            .unwrap_or_default();
            *self.catalog.write().unwrap() = harvested;
            *self.core.write().unwrap() = core;
            cached = hit;
            if progress_active {
                let n = self.catalog.read().unwrap().len();
                self.client
                    .send_notification::<notification::Progress>(ProgressParams {
                        token,
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                            WorkDoneProgressEnd {
                                message: Some(format!("{n} documented modules")),
                            },
                        )),
                    })
                    .await;
            }
            // a CONFIGURED tree that yields nothing deserves a visible
            // warning, not just a quiet log line
            if self.catalog.read().unwrap().is_empty() {
                self.client
                    .show_message(
                        MessageType::WARNING,
                        format!(
                            "kamailio-lsp: no module documentation found under '{src}' (kamailioSrc)"
                        ),
                    )
                    .await;
            }
            if let Some(w) = &wiki {
                let empty = {
                    let core = self.core.read().unwrap();
                    core.functions.is_empty() && core.params.is_empty() && core.pvars.is_empty()
                };
                if empty {
                    self.client
                        .show_message(
                            MessageType::WARNING,
                            format!(
                                "kamailio-lsp: no core documentation found under '{w}' (kamailioWiki)"
                            ),
                        )
                        .await;
                }
            }
        }
        let n = self.catalog.read().unwrap().len();
        let c = self.core.read().unwrap().functions.len();
        let tag = if cached { ", cached" } else { "" };
        self.client
            .log_message(
                MessageType::INFO,
                format!("kamailio-lsp ready ({n} documented modules, {c} core functions{tag})"),
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        let uri = p.text_document.uri;
        self.docs
            .insert(uri.clone(), (p.text_document.version, p.text_document.text));
        self.spawn_check(&uri);
    }

    async fn did_change(&self, p: DidChangeTextDocumentParams) {
        let Some(change) = p.content_changes.into_iter().last() else {
            return;
        };
        let uri = p.text_document.uri;
        let version = p.text_document.version;
        self.docs
            .insert(uri.clone(), (version, change.text.clone()));
        // debounced analyzer pass: fast feedback between saves
        if !*self.analyzer_enabled.read().unwrap() || uri.scheme() != "file" {
            return;
        }
        let generation = {
            let mut e = self.change_gen.entry(uri.clone()).or_insert(0);
            *e += 1;
            *e
        };
        let gen_map = self.change_gen.clone();
        let check_map = self.check_diags.clone();
        let cat_arc = self.catalog.clone();
        let client = self.client.clone();
        let open = self.open_docs_snapshot();
        let cap = *self.max_diagnostics.read().unwrap();
        let debounce = std::env::var("KAMAILIO_LSP_ANALYZER_DEBOUNCE_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300);
        let text = change.text;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(debounce)).await;
            // superseded by a newer edit: let the newest task publish
            if gen_map.get(&uri).map(|g| *g) != Some(generation) {
                return;
            }
            Backend::merge_and_publish(
                &client, &check_map, true, cap, &uri, version, &text, open, &cat_arc, false,
            )
            .await;
        });
    }

    async fn did_save(&self, p: DidSaveTextDocumentParams) {
        self.spawn_check(&p.text_document.uri);
    }

    async fn did_change_configuration(&self, p: DidChangeConfigurationParams) {
        // runtime-tunable settings, applied in place; the paths that
        // shape initialization (server binary, kamailio binary,
        // source/wiki trees, cache dir) still require a restart and
        // are deliberately not read here.  Settings may arrive
        // wrapped in a `kamailioLsp` section or flat; anything
        // unparseable is ignored.
        let s = &p.settings;
        let s = s.get("kamailioLsp").unwrap_or(s);
        if let Some(b) = s.get("analyzerDiagnostics").and_then(|v| v.as_bool()) {
            *self.analyzer_enabled.write().unwrap() = b;
        }
        if let Some(b) = s.get("snippetCompletions").and_then(|v| v.as_bool()) {
            *self.snippet_completions.write().unwrap() = b;
        }
        if let Some(b) = s.get("codeLensReferences").and_then(|v| v.as_bool()) {
            *self.code_lens_refs.write().unwrap() = b;
        }
        if let Some(n) = s.get("maxDiagnostics").and_then(|v| v.as_u64()) {
            *self.max_diagnostics.write().unwrap() = (n as usize).max(1);
        }
        if let Some(ms) = s.get("checkTimeoutMs").and_then(|v| v.as_u64()) {
            *self.check_timeout.write().unwrap() = logic::resolve_timeout(
                Some(ms),
                std::env::var("KAMAILIO_LSP_CHECK_TIMEOUT_MS").ok(),
            );
        }
        // retuned toggles must apply to what is already on screen:
        // republish diagnostics for every open document
        let analyzer_enabled = *self.analyzer_enabled.read().unwrap();
        let cap = *self.max_diagnostics.read().unwrap();
        let open_docs: Vec<(Url, i32, String)> = self
            .docs
            .iter()
            .map(|e| (e.key().clone(), e.value().0, e.value().1.clone()))
            .collect();
        for (uri, version, text) in open_docs {
            Self::merge_and_publish(
                &self.client,
                &self.check_diags,
                analyzer_enabled,
                cap,
                &uri,
                version,
                &text,
                self.open_docs_snapshot(),
                &self.catalog,
                false,
            )
            .await;
        }
    }

    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        if let Some((_, task)) = self.check_tasks.remove(&p.text_document.uri) {
            task.abort();
        }
        self.docs.remove(&p.text_document.uri);
        self.check_diags.remove(&p.text_document.uri);
        self.change_gen.remove(&p.text_document.uri);
    }

    async fn completion(&self, p: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = p.text_document_position.text_document.uri;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let pos = p.text_document_position.position;
        let prefix = Self::line_prefix(&text, pos);
        // typed `$` + tail: completions REPLACE the typed token (labels
        // carry the `$`, so plain insertion would double it)
        let pvar_edit_range = logic::pvar_tail(&prefix).map(|n| {
            // the tail is ASCII, so bytes == UTF-16 units
            Range {
                start: Position::new(pos.line, pos.character.saturating_sub(n as u32)),
                end: pos,
            }
        });
        let files = self.closure_for(&uri, &text);
        let cat = self.catalog.read().unwrap();
        let core = self.core.read().unwrap();
        let snippets = *self.snippet_completions.read().unwrap();
        let items: Vec<CompletionItem> =
            logic::completions_with_core_files(&cat, &core, &files, &prefix)
                .into_iter()
                .map(|c| {
                    // functions insert tabstop snippets unless disabled
                    let (insert_text, insert_text_format) =
                        if snippets && c.kind == logic::CompKind::Function {
                            if c.detail.contains("()") {
                                (
                                    Some(format!("{}()$0", c.label)),
                                    Some(InsertTextFormat::SNIPPET),
                                )
                            } else {
                                (
                                    Some(format!("{}($1)$0", c.label)),
                                    Some(InsertTextFormat::SNIPPET),
                                )
                            }
                        } else {
                            (None, None)
                        };
                    CompletionItem {
                        text_edit: pvar_edit_range.map(|range| {
                            CompletionTextEdit::Edit(TextEdit {
                                range,
                                new_text: c.label.clone(),
                            })
                        }),
                        insert_text,
                        insert_text_format,
                        label: c.label,
                        detail: Some(c.detail),
                        documentation: (!c.doc.is_empty()).then_some({
                            Documentation::MarkupContent(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: c.doc,
                            })
                        }),
                        kind: Some(match c.kind {
                            logic::CompKind::Module => CompletionItemKind::MODULE,
                            logic::CompKind::Param => CompletionItemKind::PROPERTY,
                            logic::CompKind::Function => CompletionItemKind::FUNCTION,
                            logic::CompKind::Route => CompletionItemKind::REFERENCE,
                            logic::CompKind::Keyword => CompletionItemKind::KEYWORD,
                        }),
                        ..Default::default()
                    }
                })
                .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn signature_help(&self, p: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let prefix = Self::line_prefix(&text, pos);
        let cat = self.catalog.read().unwrap();
        let core = self.core.read().unwrap();
        Ok(
            logic::signature_at(&cat, &core, &text, &prefix).map(|(sig, doc, active)| {
                // parameter labels: the signature's TOP-LEVEL
                // comma-separated pieces (nested parens/brackets and
                // quoted commas stay whole)
                let params: Vec<ParameterInformation> = logic::split_params(&sig)
                    .into_iter()
                    .map(|s| ParameterInformation {
                        label: ParameterLabel::Simple(s),
                        documentation: None,
                    })
                    .collect();
                SignatureHelp {
                    signatures: vec![SignatureInformation {
                        label: sig,
                        documentation: (!doc.is_empty()).then_some(Documentation::MarkupContent(
                            MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: doc,
                            },
                        )),
                        parameters: (!params.is_empty()).then_some(params),
                        active_parameter: Some(active),
                    }],
                    active_signature: Some(0),
                    active_parameter: Some(active),
                }
            }),
        )
    }

    async fn hover(&self, p: HoverParams) -> Result<Option<Hover>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let Some(line) = text.lines().nth(pos.line as usize) else {
            return Ok(None);
        };
        let byte_col = analyze::utf16_to_byte(line, pos.character);
        let Some(word) = analyze::word_at(line, byte_col) else {
            return Ok(None);
        };
        let cat = self.catalog.read().unwrap();
        let core = self.core.read().unwrap();
        Ok(
            logic::hover_markdown_with_core(&cat, &core, &text, &word).map(|md| Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }),
                range: None,
            }),
        )
    }

    async fn goto_definition(
        &self,
        p: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let line_text = Self::doc_line(&text, pos.line);
        let byte_col = analyze::utf16_to_byte(&line_text, pos.character) as u32;
        if let Some(d) = logic::definition_of(&text, pos.line, byte_col) {
            let def_line = Self::doc_line(&text, d.line);
            let c = analyze::byte_to_utf16(&def_line, d.col as usize);
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri,
                range: Range {
                    start: Position::new(d.line, c),
                    end: Position::new(d.line, c),
                },
            })));
        }
        // not defined in this file: search the include closure
        let Some(name) = logic::route_symbol_at(&text, pos.line, byte_col) else {
            return Ok(None);
        };
        let files = self.closure_for(&uri, &text);
        for (path, ftext) in files.iter().skip(1) {
            if let Some(d) = analyze::route_blocks(ftext)
                .into_iter()
                .filter(|b| b.kind == "route")
                .map(|b| analyze::Located {
                    name: b.name,
                    line: b.line,
                    col: b.col,
                })
                .find(|d| d.name == name)
                && let Ok(target) = Url::from_file_path(path)
            {
                let def_line = Self::doc_line(ftext, d.line);
                let c = analyze::byte_to_utf16(&def_line, d.col as usize);
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target,
                    range: Range {
                        start: Position::new(d.line, c),
                        end: Position::new(d.line, c),
                    },
                })));
            }
        }
        Ok(None)
    }

    async fn document_symbol(
        &self,
        p: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        #[allow(deprecated)]
        let syms: Vec<DocumentSymbol> = analyze::route_blocks(&text)
            .into_iter()
            .map(|b| {
                let lt = Self::doc_line(&text, b.line);
                let start = Position::new(b.line, analyze::byte_to_utf16(&lt, b.col as usize));
                let et = Self::doc_line(&text, b.end_line);
                let end =
                    Position::new(b.end_line, analyze::byte_to_utf16(&et, b.end_col as usize));
                DocumentSymbol {
                    name: if b.name.is_empty() {
                        if b.kind == "route" {
                            // legacy alias of request_route
                            "route (main)".into()
                        } else {
                            b.kind.clone()
                        }
                    } else {
                        format!("{}[{}]", b.kind, b.name)
                    },
                    detail: None,
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range: Range { start, end },
                    selection_range: Range { start, end: start },
                    children: None,
                }
            })
            .collect();
        Ok(Some(DocumentSymbolResponse::Nested(syms)))
    }

    async fn references(&self, p: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let Some((name, ns)) = Self::route_symbol_at(&text, pos) else {
            return Ok(None);
        };
        let include_decl = p.context.include_declaration;
        let root_path = uri.to_file_path().unwrap_or_default();
        let files = self.closure_for(&uri, &text);
        let mut locs = Vec::new();
        for (path, ftext) in &files {
            let Some(furi) = Self::closure_uri(&uri, &root_path, path) else {
                continue;
            };
            for (l, is_def) in logic::ns_occurrences(ftext, &name, &ns) {
                if include_decl || !is_def {
                    locs.push(Location {
                        uri: furi.clone(),
                        range: Self::occurrence_range(ftext, &l),
                    });
                }
            }
        }
        Ok(Some(locs))
    }

    async fn document_highlight(
        &self,
        p: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let Some((name, ns)) = Self::route_symbol_at(&text, pos) else {
            return Ok(None);
        };
        let hls = logic::ns_occurrences(&text, &name, &ns)
            .into_iter()
            .map(|(l, is_def)| DocumentHighlight {
                range: Self::occurrence_range(&text, &l),
                kind: Some(if is_def {
                    DocumentHighlightKind::WRITE
                } else {
                    DocumentHighlightKind::READ
                }),
            })
            .collect();
        Ok(Some(hls))
    }

    async fn prepare_rename(
        &self,
        p: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = p.text_document.uri;
        let pos = p.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let Some((name, ns)) = Self::route_symbol_at(&text, pos) else {
            // off-symbol: a null response makes the editor block F2
            return Ok(None);
        };
        // per-kind names (event_route, failure_route, ...) are never
        // renamable — rename() would reject them, so block at prepare
        if !matches!(ns, logic::RouteNs::Main) {
            return Ok(None);
        }
        let line = Self::doc_line(&text, pos.line);
        let byte_col = analyze::utf16_to_byte(&line, pos.character) as u32;
        let occ = logic::ns_occurrences(&text, &name, &ns)
            .into_iter()
            .find(|(l, _)| {
                l.line == pos.line && byte_col >= l.col && byte_col < l.col + name.len() as u32
            });
        let Some((l, _)) = occ else {
            return Ok(None);
        };
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: Self::occurrence_range(&text, &l),
            placeholder: name,
        }))
    }

    async fn document_link(&self, p: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = p.text_document.uri;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let base_dir = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        let links: Vec<DocumentLink> = analyze::include_links(&text)
            .into_iter()
            .filter_map(|l| {
                let p = std::path::Path::new(&l.path);
                // resolution parity with the checker and the closure
                // walker: absolute as written, relative against the
                // document's own directory.  Missing files still get
                // a link — the editor surfaces the miss on click.
                let target = if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    base_dir.as_ref()?.join(p)
                };
                let target = Url::from_file_path(&target).ok()?;
                let lt = Self::doc_line(&text, l.line);
                let s = analyze::byte_to_utf16(&lt, l.col as usize);
                let e = analyze::byte_to_utf16(&lt, (l.col + l.len) as usize);
                Some(DocumentLink {
                    range: Range {
                        start: Position::new(l.line, s),
                        end: Position::new(l.line, e),
                    },
                    target: Some(target),
                    tooltip: None,
                    data: None,
                })
            })
            .collect();
        Ok(Some(links))
    }

    async fn rename(&self, p: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let new_name = p.new_name;
        if !logic::valid_route_name(&new_name) {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                "'{new_name}' is not a legal unquoted route name ([A-Za-z_][A-Za-z0-9_]*)"
            )));
        }
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let Some((name, ns)) = Self::route_symbol_at(&text, pos) else {
            return Ok(None);
        };
        match &ns {
            logic::RouteNs::Kind(k) if k == "event_route" => {
                // the name is the module's event identifier
                return Err(tower_lsp::jsonrpc::Error::invalid_params(
                    "event route names are defined by their module and cannot be renamed",
                ));
            }
            logic::RouteNs::Kind(k) => {
                // armed through module functions (t_on_failure, ...)
                // whose string arguments we cannot rewrite safely
                return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                    "{k} names are referenced from module-function arguments; renaming here would not update those call sites"
                )));
            }
            logic::RouteNs::Main => {}
        }
        // rewrite every occurrence in the whole include closure
        let root_path = uri.to_file_path().unwrap_or_default();
        let files = self.closure_for(&uri, &text);
        let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();
        for (path, ftext) in &files {
            let Some(furi) = Self::closure_uri(&uri, &root_path, path) else {
                continue;
            };
            let edits: Vec<TextEdit> = logic::ns_occurrences(ftext, &name, &ns)
                .into_iter()
                .map(|(l, _)| TextEdit {
                    range: Self::occurrence_range(ftext, &l),
                    new_text: new_name.clone(),
                })
                .collect();
            if !edits.is_empty() {
                changes.insert(furi, edits);
            }
        }
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }))
    }

    async fn symbol(&self, p: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
        let query = p.query.to_lowercase();
        let mut seen: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();
        let mut out = Vec::new();
        // every open document plus its include closure, deduplicated
        let open: Vec<(Url, String)> = self
            .docs
            .iter()
            .map(|e| (e.key().clone(), e.value().1.clone()))
            .collect();
        for (uri, text) in open {
            for (path, ftext) in self.closure_for(&uri, &text) {
                if !seen.insert(path.clone()) {
                    continue;
                }
                let furi = if path.as_os_str().is_empty() {
                    uri.clone()
                } else {
                    match Url::from_file_path(&path) {
                        Ok(u) => u,
                        Err(_) => continue,
                    }
                };
                for b in analyze::route_blocks(&ftext) {
                    if b.name.is_empty() || !b.name.to_lowercase().contains(&query) {
                        continue;
                    }
                    let lt = Self::doc_line(&ftext, b.line);
                    let c = analyze::byte_to_utf16(&lt, b.col as usize);
                    #[allow(deprecated)]
                    out.push(SymbolInformation {
                        name: format!("{}[{}]", b.kind, b.name),
                        kind: SymbolKind::FUNCTION,
                        tags: None,
                        deprecated: None,
                        location: Location {
                            uri: furi.clone(),
                            range: Range {
                                start: Position::new(b.line, c),
                                end: Position::new(b.line, c),
                            },
                        },
                        container_name: None,
                    });
                    if out.len() >= 256 {
                        return Ok(Some(out));
                    }
                }
            }
        }
        Ok(Some(out))
    }

    async fn semantic_tokens_full(
        &self,
        p: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let data = logic::encode_semantic_tokens(&text)
            .chunks(5)
            .map(|c| SemanticToken {
                delta_line: c[0],
                delta_start: c[1],
                length: c[2],
                token_type: c[3],
                token_modifiers_bitset: c[4],
            })
            .collect();
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn semantic_tokens_range(
        &self,
        p: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let r = p.range;
        let data = logic::encode_semantic_tokens_range(
            &text,
            (r.start.line, r.start.character),
            (r.end.line, r.end.character),
        )
        .chunks(5)
        .map(|c| SemanticToken {
            delta_line: c[0],
            delta_start: c[1],
            length: c[2],
            token_type: c[3],
            token_modifiers_bitset: c[4],
        })
        .collect();
        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn code_action(&self, p: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = p.text_document.uri;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let cat = self.catalog.read().unwrap();
        let mut out: Vec<CodeActionOrCommand> = Vec::new();
        for d in &p.context.diagnostics {
            // the fix locator needs the BYTE column of the diagnostic
            let dl = Self::doc_line(&text, d.range.start.line);
            let byte_col = analyze::utf16_to_byte(&dl, d.range.start.character) as u32;
            for f in logic::quick_fixes(&cat, &text, &d.message, d.range.start.line, byte_col) {
                let pos = Position::new(f.line, f.col);
                let mut changes = std::collections::HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit {
                        range: Range {
                            start: pos,
                            end: pos,
                        },
                        new_text: f.insert,
                    }],
                );
                out.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: f.title,
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![d.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
        }
        Ok(Some(out))
    }

    async fn code_lens(&self, p: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        if !*self.code_lens_refs.read().unwrap() {
            return Ok(None);
        }
        let uri = p.text_document.uri;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let files = self.closure_for(&uri, &text);
        let lenses = analyze::route_blocks(&text)
            .into_iter()
            // only main-table (kind "route") names are route()-callable;
            // other kinds are armed via module functions we don't track,
            // so a count would mislead
            .filter(|b| b.kind == "route" && !b.name.is_empty())
            .map(|b| {
                let n: usize = files
                    .iter()
                    .map(|(_, t)| {
                        analyze::route_refs(t)
                            .into_iter()
                            .filter(|r| r.name == b.name)
                            .count()
                    })
                    .sum();
                let lt = Self::doc_line(&text, b.name_line);
                let c = analyze::byte_to_utf16(&lt, b.name_col as usize);
                CodeLens {
                    range: Range {
                        start: Position::new(b.name_line, c),
                        end: Position::new(b.name_line, c),
                    },
                    command: Some(Command {
                        title: if n == 1 {
                            "1 reference".into()
                        } else {
                            format!("{n} references")
                        },
                        command: String::new(),
                        arguments: None,
                    }),
                    data: None,
                }
            })
            .collect();
        Ok(Some(lenses))
    }

    async fn folding_range(&self, p: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let ranges: Vec<FoldingRange> = analyze::route_blocks(&text)
            .into_iter()
            .filter(|b| b.end_line > b.line)
            .map(|b| FoldingRange {
                start_line: b.line,
                start_character: None,
                end_line: b.end_line,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: None,
            })
            .collect();
        Ok(Some(ranges))
    }
}
