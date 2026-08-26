//! The tower-lsp-server language server.

use dashmap::DashMap;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer};

use crate::{analyze, catalog, diag, logic};

/// Parameters of the `kamailio/analysisRoot` request.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnalysisRootParams {
    /// The document to locate: `file:` URIs only.
    pub uri: Uri,
}

/// What the server is judging `modparam` names against, pushed to the
/// client so the editor can show it.
///
/// A warning names the catalogue, but only once something is wrong.
/// Until then nothing tells the reader which release their file is
/// being parsed against, and the answer changes with a setting they
/// may not have set themselves.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogueStatus {
    /// The catalogue named as it reads in a sentence, e.g.
    /// `Kamailio 6.1.4 (built in)` or `the configured source tree`.
    pub describe: String,
    /// The release, when it is a built-in one. Absent for a
    /// configured tree, which is exact for the user's own build and
    /// carries no version this server can name.
    pub version: Option<String>,
}

/// `kamailioLsp/catalogue`: sent once the catalogue is settled, and
/// again whenever it changes.
pub enum CatalogueNotification {}

impl tower_lsp_server::ls_types::notification::Notification for CatalogueNotification {
    type Params = CatalogueStatus;
    const METHOD: &'static str = "kamailioLsp/catalogue";
}

/// LSP backend: document store, doc catalog, and the `-c` runner.
pub struct Backend {
    client: Client,
    /// Open documents: (version, full text).
    docs: std::sync::Arc<DashMap<Uri, (i32, String)>>,
    catalog: std::sync::Arc<std::sync::RwLock<Vec<catalog::ModuleDoc>>>,
    /// Which catalogue the modparam check judges against, so a
    /// diagnostic can name it. What a module exports moves between
    /// releases, and a parameter absent from a built-in catalogue
    /// may simply be one that version does not have.
    catalog_origin: std::sync::Arc<std::sync::RwLock<catalog::CatalogOrigin>>,
    /// The built-in release to check against, when the user named
    /// one. A configured source tree still wins over it: that is
    /// exact for their build, and this is a choice among ours.
    wanted_version: std::sync::RwLock<Option<String>>,
    /// Whether built-in documentation repeats the release it came
    /// from, under every hover and completion item. Off by default:
    /// the status bar says it continuously and every warning that
    /// turns on the release names it.
    version_in_hints: std::sync::RwLock<bool>,
    /// Hovers and completion answer at all. A reader walking someone
    /// else's configuration turns the popups off with one key and
    /// back on with the same one; it is pushed live, because a
    /// setting that needs a restart is a pause and not a toggle.
    assistance: std::sync::RwLock<bool>,
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
    check_tasks: DashMap<Uri, tokio::task::JoinHandle<()>>,
    snippet_completions: std::sync::RwLock<bool>,
    /// Draw parameter names at documented call sites.
    inlay_parameter_names: std::sync::RwLock<bool>,
    /// The client accepts dynamic `didChangeWatchedFiles` registration.
    watched_files_dynamic: std::sync::RwLock<bool>,
    /// The client pulls diagnostics; pushing as well would double-report.
    diagnostics_pulled: std::sync::Arc<std::sync::RwLock<bool>>,
    /// The client can be told to re-pull after an async check lands.
    diagnostics_refresh: std::sync::RwLock<bool>,
    /// Workspace roots, for workspace-wide diagnostics.
    workspace_roots: std::sync::RwLock<Vec<std::path::PathBuf>>,
    /// Draw what each preprocessor symbol expands to.
    inlay_define_values: std::sync::RwLock<bool>,
    max_diagnostics: std::sync::RwLock<usize>,
    cache_dir_opt: std::sync::RwLock<Option<String>>,
    check_timeout: std::sync::RwLock<std::time::Duration>,
    /// Last published `-c` results per document; merged with analyzer
    /// diagnostics on every publish.
    check_diags: std::sync::Arc<DashMap<Uri, Vec<Diagnostic>>>,
    /// Fast analyzer diagnostics between saves (init option).
    analyzer_enabled: std::sync::RwLock<bool>,
    /// didChange generation per document: only the latest debounced
    /// analyzer task publishes.
    change_gen: std::sync::Arc<DashMap<Uri, u64>>,
    /// Reference-count code lenses on route definitions (init option).
    code_lens_refs: std::sync::RwLock<bool>,
    /// Did the client advertise window.workDoneProgress support?
    work_done_progress: std::sync::RwLock<bool>,
    /// Per-(URI, version) memo of the per-document computations the
    /// hot handlers share (blocks, refs, semantic spans).
    doc_index: logic::DocCache<Uri>,
    /// The workspace's include graph, inverted, so an open FRAGMENT
    /// can be answered in the context of the root that includes it.
    /// Built on demand and dropped whenever an include directive
    /// anywhere could have moved.
    include_graph: std::sync::RwLock<Option<std::sync::Arc<logic::IncludeGraph>>>,
    /// Whether the bounded scan behind that graph has already been
    /// reported as incomplete.  Said once: the graph is rebuilt often
    /// and the same line every time would bury the log it is in.
    graph_truncation_logged: std::sync::atomic::AtomicBool,
    /// Whether a config the graph could not read has been reported.
    unreadable_logged: std::sync::atomic::AtomicBool,
}

impl Backend {
    /// Read the assistance toggle out of a settings object.
    ///
    /// This server reads its settings in two places — the
    /// initialization options and the live configuration change — so
    /// the toggle is written once and called from both. Written twice
    /// it would drift, and a toggle that works on startup but not
    /// live is the one case it exists to cover.
    ///
    /// An absent value means unchanged: an editor that sends only
    /// what changed must not turn the popups back on by way of an
    /// unrelated setting.
    fn apply_assistance(&self, opts: &serde_json::Value) {
        if let Some(b) = opts.get("assistance").and_then(|v| v.as_bool()) {
            *self.assistance.write().unwrap() = b;
        } else if let Ok(v) = std::env::var("KAMAILIO_LSP_ASSISTANCE") {
            *self.assistance.write().unwrap() = !(v == "0" || v.eq_ignore_ascii_case("false"));
        }
    }

    /// Build a backend for one client connection.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: std::sync::Arc::new(DashMap::new()),
            catalog: std::sync::Arc::new(std::sync::RwLock::new(Vec::new())),
            catalog_origin: std::sync::Arc::new(std::sync::RwLock::new(
                catalog::CatalogOrigin::BuiltIn(catalog::builtin_modules().version.clone()),
            )),
            wanted_version: std::sync::RwLock::new(None),
            version_in_hints: std::sync::RwLock::new(false),
            assistance: std::sync::RwLock::new(true),
            core: std::sync::RwLock::new(catalog::CoreDocs::default()),
            src: std::sync::RwLock::new(None),
            wiki: std::sync::RwLock::new(None),
            modules_path: std::sync::RwLock::new(None),
            kamailio_bin: std::sync::RwLock::new(None),
            check_gate: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            check_tasks: DashMap::new(),
            snippet_completions: std::sync::RwLock::new(true),
            inlay_parameter_names: std::sync::RwLock::new(true),
            watched_files_dynamic: std::sync::RwLock::new(false),
            diagnostics_pulled: std::sync::Arc::new(std::sync::RwLock::new(false)),
            diagnostics_refresh: std::sync::RwLock::new(false),
            workspace_roots: std::sync::RwLock::new(Vec::new()),
            inlay_define_values: std::sync::RwLock::new(true),
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
            doc_index: logic::DocCache::new(),
            include_graph: std::sync::RwLock::new(None),
            graph_truncation_logged: std::sync::atomic::AtomicBool::new(false),
            unreadable_logged: std::sync::atomic::AtomicBool::new(false),
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
        docs: &DashMap<Uri, (i32, String)>,
    ) -> std::collections::HashMap<std::path::PathBuf, String> {
        docs.iter()
            .filter_map(|e| {
                let url = e.key();
                if url.scheme().as_str() != "file" {
                    return None;
                }
                url.to_file_path()
                    .map(|p| (p.into_owned(), e.value().1.clone()))
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
            logic::read_config(p)
        }
    }

    /// The include closure a document is answered in, given the root
    /// it belongs to.
    ///
    /// A root is its own closure.  An included FRAGMENT is answered in
    /// its ROOT's closure: the routes, modules and defines the parent
    /// brings are part of the one program the fragment is a piece of,
    /// and without them every one of them reads as undefined.  The
    /// current document is placed first — the completion engine reads
    /// the closure in that order — and its own entry is the caller's
    /// buffer, which is newer than anything on disk.
    ///
    /// Static so a spawned check task can call it without `&self`.
    fn closure_rooted_at(
        root: Option<&std::path::Path>,
        path: &std::path::Path,
        text: &str,
        loader: &dyn Fn(&std::path::Path) -> Option<String>,
    ) -> Vec<(std::path::PathBuf, String)> {
        let mut files = match root.and_then(|r| loader(r).map(|t| (r.to_path_buf(), t))) {
            Some((r, rt)) => logic::include_closure(&r, &rt, loader),
            // no root, or a root that has gone: the document's own
            // closure is the best context there is
            None => logic::include_closure(path, text, loader),
        };
        match files.iter().position(|(p, _)| p == path) {
            Some(i) => {
                files[i].1 = text.to_string();
                let own = files.remove(i);
                files.insert(0, own);
                files
            }
            // The document is not IN the closure built from its root:
            // the closure is capped (depth 8, 64 files) and a
            // configuration with one include per carrier passes 64
            // without trying.  Adding the document alone here would
            // drop ITS includes — routes that were in scope before the
            // root was ever consulted.  Analysing in the root's
            // context must only ADD to what the file could already
            // see, so its own closure leads and the root's follows.
            None => {
                let mut merged = logic::include_closure(path, text, loader);
                for (p, t) in files {
                    if !merged.iter().any(|(q, _)| *q == p) {
                        merged.push((p, t));
                    }
                }
                merged
            }
        }
    }

    /// Analyzer diagnostics for `text`, mapped to LSP (UTF-16) ranges.
    fn analyzer_lsp_diags(
        files: &[(std::path::PathBuf, String)],
        path: &std::path::Path,
        text: &str,
        cat: &[catalog::ModuleDoc],
        origin: &catalog::CatalogOrigin,
    ) -> Vec<Diagnostic> {
        let mut all = logic::analyzer_diagnostics_in_closure(files, path, text);
        all.extend(logic::catalog_diagnostics(cat, origin, text));
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
        pushing: bool,
        client: &Client,
        check_map: &DashMap<Uri, Vec<Diagnostic>>,
        analyzer_enabled: bool,
        cap: usize,
        uri: &Uri,
        version: i32,
        text: &str,
        open: std::collections::HashMap<std::path::PathBuf, String>,
        cat: &std::sync::Arc<std::sync::RwLock<Vec<catalog::ModuleDoc>>>,
        origin: &std::sync::Arc<std::sync::RwLock<catalog::CatalogOrigin>>,
        skip_if_empty: bool,
        root: Option<&std::path::Path>,
    ) {
        // a client that pulls gets everything through
        // `textDocument/diagnostic`; pushing as well shows each
        // problem twice
        if !pushing {
            return;
        }
        let merged = Self::merged_diags(
            check_map,
            analyzer_enabled,
            cap,
            uri,
            text,
            open,
            cat,
            origin,
            root,
        );
        if merged.is_empty() && skip_if_empty {
            return;
        }
        client
            .publish_diagnostics(uri.clone(), merged, Some(version))
            .await;
    }

    /// The diagnostics for one document: the last checker result plus
    /// a fresh analyzer pass, capped.  Shared by the push path and the
    /// pull one so both can never disagree about what is wrong.
    #[allow(clippy::too_many_arguments)]
    fn merged_diags(
        check_map: &DashMap<Uri, Vec<Diagnostic>>,
        analyzer_enabled: bool,
        cap: usize,
        uri: &Uri,
        text: &str,
        open: std::collections::HashMap<std::path::PathBuf, String>,
        cat: &std::sync::Arc<std::sync::RwLock<Vec<catalog::ModuleDoc>>>,
        origin: &std::sync::Arc<std::sync::RwLock<catalog::CatalogOrigin>>,
        root: Option<&std::path::Path>,
    ) -> Vec<Diagnostic> {
        let mut merged = check_map.get(uri).map(|v| v.clone()).unwrap_or_default();
        if analyzer_enabled && let Some(path) = uri.to_file_path() {
            let loader = Self::make_loader(open);
            let cat = cat.read().unwrap().clone();
            let files = Self::closure_rooted_at(root, &path, text, &loader);
            let origin = origin.read().unwrap().clone();
            merged.extend(Self::analyzer_lsp_diags(&files, &path, text, &cat, &origin));
        }
        merged.truncate(cap.max(1));
        merged
    }

    /// Whole-line rewrites as LSP edits.  Each edit replaces exactly
    /// one line's content and never its newline, so an edit list can
    /// be applied in any order and the document's line structure is
    /// untouched.
    fn line_edits(text: &str, edits: Vec<crate::format::LineEdit>) -> Vec<TextEdit> {
        edits
            .into_iter()
            .map(|e| {
                let old = Self::doc_line(text, e.line);
                TextEdit {
                    range: Range {
                        start: Position::new(e.line, 0),
                        end: Position::new(e.line, analyze::byte_to_utf16(&old, old.len())),
                    },
                    new_text: e.text,
                }
            })
            .collect()
    }

    /// The open buffer for `uri`, or empty when the document is not
    /// open — a call-hierarchy follow-up can name a file the client
    /// never opened.
    fn text_for(&self, uri: &Uri) -> String {
        if let Some(d) = self.docs.get(uri) {
            return d.1.clone();
        }
        // Not open — and for a call-hierarchy follow-up that is the
        // NORMAL case: the item names the file a route is DEFINED in,
        // which is the root while the user is editing an include.
        // Reading it as empty makes its closure empty and the call
        // graph with it, so "nobody calls this" comes back for a route
        // the buffer on screen calls two lines up.  Same cap as the
        // include loader: a hostile file must stay cheap.
        let Some(path) = uri.to_file_path() else {
            return String::new();
        };
        match std::fs::metadata(&path) {
            Ok(m) if m.is_file() && m.len() <= 1_048_576 => {
                std::fs::read_to_string(&path).unwrap_or_default()
            }
            _ => String::new(),
        }
    }

    /// A call-hierarchy item for one route block.
    ///
    /// `range` is the block's whole extent so an editor can frame it;
    /// `selection_range` is the name itself, which is what gets
    /// highlighted on navigation.  `data` carries the route name so
    /// the follow-up calls need not re-derive it from a position.
    fn hierarchy_item(uri: &Uri, b: &analyze::Block, text: &str) -> CallHierarchyItem {
        let name_line = Self::doc_line(text, b.name_line);
        let name_start = analyze::byte_to_utf16(&name_line, b.name_col as usize);
        let kw_line = Self::doc_line(text, b.line);
        let end_line = Self::doc_line(text, b.end_line);
        CallHierarchyItem {
            name: if b.name.is_empty() {
                b.kind.clone()
            } else {
                format!("{}[{}]", b.kind, b.name)
            },
            kind: SymbolKind::FUNCTION,
            tags: None,
            detail: None,
            uri: uri.clone(),
            range: Range {
                start: Position::new(b.line, analyze::byte_to_utf16(&kw_line, b.col as usize)),
                end: Position::new(
                    b.end_line,
                    analyze::byte_to_utf16(&end_line, b.end_col as usize),
                ),
            },
            selection_range: Range {
                start: Position::new(b.name_line, name_start),
                end: Position::new(
                    b.name_line,
                    name_start + analyze::byte_to_utf16(&b.name, b.name.len()),
                ),
            },
            data: Some(serde_json::json!({ "route": b.name })),
        }
    }

    /// The UTF-16 range of one `route(NAME)` call site's name.
    fn call_range(text: &str, call: &analyze::Located) -> Range {
        let line = Self::doc_line(text, call.line);
        let start = analyze::byte_to_utf16(&line, call.col as usize);
        Range {
            start: Position::new(call.line, start),
            end: Position::new(
                call.line,
                start + analyze::byte_to_utf16(&call.name, call.name.len()),
            ),
        }
    }

    /// The route name a call-hierarchy item stands for.
    fn item_route(item: &CallHierarchyItem) -> Option<String> {
        item.data
            .as_ref()
            .and_then(|d| d.get("route"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    }

    /// The main-table definition of `name` within an include closure,
    /// as (uri, text, block).
    fn main_definition(
        files: &[(std::path::PathBuf, String)],
        name: &str,
    ) -> Option<(Uri, String, analyze::Block)> {
        for (path, text) in files {
            let Some(uri) = Uri::from_file_path(path) else {
                continue;
            };
            if let Some(b) = analyze::route_blocks(text)
                .into_iter()
                .find(|b| b.kind == "route" && b.name == name)
            {
                return Some((uri, text.clone(), b));
            }
        }
        None
    }

    /// A stable identity for one document's diagnostics, so an
    /// unchanged report can say "unchanged" instead of resending.
    fn result_id(diags: &[Diagnostic]) -> String {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for d in diags {
            d.range.start.line.hash(&mut h);
            d.range.start.character.hash(&mut h);
            d.range.end.line.hash(&mut h);
            d.range.end.character.hash(&mut h);
            d.message.hash(&mut h);
        }
        format!("{:x}", h.finish())
    }

    /// Every `*.cfg` under the workspace roots, bounded.
    ///
    /// The bound is announced rather than silent: a truncated sweep
    /// that looks complete is worse than one that says it stopped.
    fn workspace_configs(&self, limit: usize) -> (Vec<std::path::PathBuf>, bool) {
        logic::scan_configs(&self.workspace_roots.read().unwrap(), limit)
    }

    /// Ask the client to watch the files whose contents this server
    /// derives answers from.
    ///
    /// Three kinds matter and none arrives as a document edit: a
    /// config included by an open file, the module documentation tree,
    /// and the wiki checkout the core docs come from.  Without this the
    /// server keeps answering from a stale read until the buffer
    /// happens to be touched.
    ///
    /// Registration is dynamic because tree and wiki live wherever the
    /// user put them — usually outside the workspace — so those
    /// watchers are relative patterns rooted at each.
    async fn register_watchers(&self) {
        // registering against a client that never declared support is
        // a request that may never be answered
        if !*self.watched_files_dynamic.read().unwrap() {
            return;
        }
        let mut watchers = vec![FileSystemWatcher {
            glob_pattern: GlobPattern::String("**/*.cfg".into()),
            kind: None,
        }];
        let mut relative = |dir: Option<String>, pats: &[&str]| {
            if let Some(d) = dir
                && let Some(base) = Uri::from_file_path(&d)
            {
                for pat in pats {
                    watchers.push(FileSystemWatcher {
                        glob_pattern: GlobPattern::Relative(RelativePattern {
                            base_uri: OneOf::Right(base.clone()),
                            pattern: (*pat).into(),
                        }),
                        kind: None,
                    });
                }
            }
        };
        relative(self.src.read().unwrap().clone(), &["src/modules/*/README"]);
        relative(
            self.wiki.read().unwrap().clone(),
            &["docs/cookbooks/*/*.md"],
        );
        let reg = Registration {
            id: "kamailio-lsp/watched-files".into(),
            method: "workspace/didChangeWatchedFiles".into(),
            register_options: serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                watchers,
            })
            .ok(),
        };
        // time-bounded like the progress round-trip: a client that
        // declares support but never answers must not stall startup,
        // and a decline is not an error worth surfacing
        let _ = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            self.client.register_capability(vec![reg]),
        )
        .await;
    }

    /// Harvest the documentation catalogue from the configured tree
    /// and wiki checkout, replacing what is loaded, and report whether
    /// the cache answered.
    ///
    /// Startup and the file watcher share this: a tree that changes on
    /// disk has to be re-read, and the cache fingerprint is
    /// content-aware, so a changed file misses it by construction
    /// rather than by any special casing here.
    async fn harvest(&self, with_progress: bool) -> bool {
        let src = self.src.read().unwrap().clone();
        let wiki = self.wiki.read().unwrap().clone();
        let mut cached = false;
        if let Some(src) = src {
            // progress reporting: only for clients that advertised
            // window.workDoneProgress, and only if create succeeds
            let token = NumberOrString::String("kamailio-lsp/harvest".into());
            let progress_active = with_progress
                && *self.work_done_progress.read().unwrap()
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
                let mut core = wiki_path.map(catalog::harvest_core).unwrap_or_default();
                // the wiki documents the language; the levels `xlog`
                // takes are a switch in the module source beside it
                core.log_levels = catalog::harvest_log_levels(p);
                // both socket syntaxes: membership from this tree's
                // grammar, descriptions from the wiki beside it
                if let Some(w) = wiki_path {
                    let (attrs, mods) = catalog::harvest_socket_syntax(p, w);
                    core.socket_attrs = attrs;
                    core.listen_modifiers = mods;
                }
                let out = (catalog::harvest_tree(p), core);
                if let Some(dir) = &cache_dir {
                    let _ = catalog::save_cache(p, wiki_path, dir, &out.0, &out.1);
                }
                (out.0, out.1, false)
            })
            .await
            .unwrap_or_default();
            // a tree the user configured is exact for their build,
            // so its diagnostics name the tree rather than a version
            if !harvested.is_empty() {
                *self.catalog_origin.write().unwrap() = catalog::CatalogOrigin::ConfiguredTree;
            }
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
        cached
    }

    /// The workspace's include graph, from a bounded scan, cached
    /// until something that could move an include directive happens.
    ///
    /// Rebuilding this on every keystroke would read the whole
    /// workspace per character; never rebuilding it would pin a
    /// fragment to a parent it no longer has.  The invalidation points
    /// are [`Self::invalidate_include_graph`]'s callers.
    fn include_graph(&self) -> std::sync::Arc<logic::IncludeGraph> {
        if let Some(g) = self.include_graph.read().unwrap().clone() {
            return g;
        }
        let (configs, truncated) = self.workspace_configs(500);
        // A silent bound is the worst of the three options: the
        // fragment stops being recognised and nothing anywhere says
        // why.  `include_graph` is not async, so the line goes out on
        // its own task.
        if truncated
            && !self
                .graph_truncation_logged
                .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            let client = self.client.clone();
            tokio::spawn(async move {
                client
                    .log_message(
                        MessageType::WARNING,
                        "kamailio-lsp: the include graph was built from the first 500 configs \
                         under the workspace; a file included by a config outside that \
                         set will not be recognised as part of it",
                    )
                    .await;
            });
        }
        let open = self.open_docs_snapshot();
        let mut unreadable: Vec<std::path::PathBuf> = Vec::new();
        let scanned: Vec<(std::path::PathBuf, String)> = configs
            .into_iter()
            .filter_map(|p| {
                // the same reading the closure will do, so the graph
                // cannot hand out a root the closure then refuses
                let text = match open.get(&p) {
                    Some(t) => t.clone(),
                    None => match logic::read_config(&p) {
                        Some(t) => t,
                        None => {
                            unreadable.push(p);
                            return None;
                        }
                    },
                };
                Some((p, text))
            })
            .collect();
        // Same principle as the scan bound: a config the graph could
        // not read is a config whose fragments quietly stop being
        // recognised, so it is named rather than dropped in silence.
        if !unreadable.is_empty()
            && !self
                .unreadable_logged
                .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            let client = self.client.clone();
            let names: Vec<String> = unreadable
                .iter()
                .take(5)
                .map(|p| p.display().to_string())
                .collect();
            let n = unreadable.len();
            tokio::spawn(async move {
                client
                    .log_message(
                        MessageType::WARNING,
                        format!(
                            "kamailio-lsp: {n} config(s) could not be read for the include \
                             graph — too large (over 1 MiB) or not readable; files \
                             they include will not be recognised as part of them: {}",
                            names.join(", ")
                        ),
                    )
                    .await;
            });
        }
        let g = std::sync::Arc::new(logic::IncludeGraph::build(&scanned));
        *self.include_graph.write().unwrap() = Some(g.clone());
        g
    }

    /// `kamailio/analysisRoot`: the config the given document is
    /// analysed as part of, or `null` when it is a program in its own
    /// right — or a file this workspace has never included.
    ///
    /// This is how an editor tells a piece of a kamailio configuration
    /// from any other `.cfg` on disk.  The extension cannot claim
    /// `*.cfg` statically without hijacking every unrelated config
    /// file in the workspace, and an included fragment is rarely named
    /// so a filename pattern could catch it — but the configuration
    /// that includes it says exactly what it is.
    pub async fn analysis_root(&self, p: AnalysisRootParams) -> Result<Option<String>> {
        Ok(self
            .analysis_root_of(&p.uri)
            .and_then(Uri::from_file_path)
            .map(|u| u.as_str().to_string()))
    }

    /// Drop the cached include graph; the next question rebuilds it.
    fn invalidate_include_graph(&self) {
        *self.include_graph.write().unwrap() = None;
    }

    /// The config `uri` must be analysed as part of, or `None` when it
    /// is a program in its own right (or the workspace never saw it).
    fn analysis_root_of(&self, uri: &Uri) -> Option<std::path::PathBuf> {
        let path = uri.to_file_path()?;
        self.include_graph().analysis_root(&path)
    }

    /// The include closure an open document is answered in: its own
    /// when it is a root, its ROOT's when it is an included fragment
    /// (open buffers first, disk fallback).  Non-file documents get a
    /// single-entry closure.
    fn closure_for(&self, uri: &Uri, text: &str) -> Vec<(std::path::PathBuf, String)> {
        let Some(path) = uri.to_file_path() else {
            return vec![(std::path::PathBuf::new(), text.to_string())];
        };
        let loader = Self::make_loader(self.open_docs_snapshot());
        Self::closure_rooted_at(self.analysis_root_of(uri).as_deref(), &path, text, &loader)
    }

    /// The open document's (version, text, memoized index), or None
    /// if the document is not open.  KAMAILIO_LSP_TRACE_INDEX=1
    /// writes a stderr line per actual index build (test seam).
    fn doc_with_index(&self, uri: &Uri) -> Option<(i32, String, std::sync::Arc<logic::DocIndex>)> {
        let (version, text) = self.docs.get(uri).map(|d| d.clone())?;
        let before = logic::doc_index_builds();
        let idx = self.doc_index.get_or_index(uri.clone(), version, &text);
        if logic::doc_index_builds() != before
            && std::env::var("KAMAILIO_LSP_TRACE_INDEX").is_ok_and(|v| !v.is_empty())
        {
            eprintln!("kamailio-lsp: index build {} v{version}", uri.as_str());
        }
        Some((version, text, idx))
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
        root_uri: &Uri,
        root_path: &std::path::Path,
        p: &std::path::Path,
    ) -> Option<Uri> {
        if p == root_path {
            Some(root_uri.clone())
        } else {
            Uri::from_file_path(p)
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
    fn spawn_check(&self, uri: &Uri) {
        self.spawn_check_publishing(uri, true);
    }

    /// As [`Self::spawn_check`], but able to publish a clean result.
    fn spawn_check_publishing(&self, uri: &Uri, quiet_when_clean: bool) {
        if uri.scheme().as_str() != "file" {
            return;
        }
        let ctx = CheckCtx {
            client: self.client.clone(),
            docs: self.docs.clone(),
            catalog: self.catalog.clone(),
            catalog_origin: self.catalog_origin.clone(),
            check_diags: self.check_diags.clone(),
            check_gate: self.check_gate.clone(),
            bin: self.kamailio_bin.read().unwrap().clone(),
            modules_path: self.modules_path.read().unwrap().clone(),
            check_timeout: *self.check_timeout.read().unwrap(),
            analyzer_enabled: *self.analyzer_enabled.read().unwrap(),
            cap: *self.max_diagnostics.read().unwrap(),
            uri: uri.clone(),
            root: self.analysis_root_of(uri),
            pushing: !*self.diagnostics_pulled.read().unwrap(),
            refresh_after: *self.diagnostics_pulled.read().unwrap()
                && *self.diagnostics_refresh.read().unwrap(),
            quiet_when_clean,
        };
        if let Some((_, old)) = self.check_tasks.remove(uri) {
            old.abort();
        }
        self.check_tasks
            .insert(uri.clone(), tokio::spawn(Self::run_check(ctx)));
    }

    /// One check task: run it, then — for a pulling client — invite a
    /// re-ask.
    ///
    /// The checker is asynchronous, so a pulling client was already
    /// answered from the previous result; without the invitation that
    /// stale answer would be the last word.
    async fn run_check(ctx: CheckCtx) {
        let (client, refresh) = (ctx.client.clone(), ctx.refresh_after);
        Self::run_check_inner(ctx).await;
        if refresh {
            let _ = client
                .send_request::<request::WorkspaceDiagnosticRefresh>(())
                .await;
        }
    }

    /// The body of one check task; state was snapshotted at spawn.
    async fn run_check_inner(ctx: CheckCtx) {
        let uri = &ctx.uri;
        let Some(path) = uri.to_file_path() else {
            return;
        };
        let path_str = path.display().to_string();
        // The checker is handed a PROGRAM.  When the document on
        // screen is an included fragment that program is its root, not
        // the fragment: checked on its own a fragment reports every
        // construct it continues as a syntax error.
        let check_path: std::path::PathBuf = match &ctx.root {
            Some(r) => r.clone(),
            None => path.to_path_buf(),
        };
        let check_path_str = check_path.display().to_string();
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
            // -c disabled: analyzer-only pass
            ctx.check_diags.insert(uri.clone(), Vec::new());
            Self::merge_and_publish(
                ctx.pushing,
                &ctx.client,
                &ctx.check_diags,
                analyzer_enabled,
                cap,
                uri,
                snap_version,
                &snap_text,
                Self::open_docs_snapshot_of(&ctx.docs),
                &ctx.catalog,
                &ctx.catalog_origin,
                ctx.quiet_when_clean,
                ctx.root.as_deref(),
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
            cmd.current_dir(check_path.parent().unwrap_or(std::path::Path::new(".")));
            cmd.arg("-c").arg("--all-errors");
            cmd.arg("-Y").arg(std::env::temp_dir());
            if let Some(mp) = &modules_path {
                cmd.arg("-L").arg(mp);
            }
            let mut child = cmd
                .arg("-f")
                .arg(&check_path_str)
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
                            "kamailio-lsp: '{bin} -c' timed out after {:?} on {check_path_str}",
                            check_timeout
                        ),
                    )
                    .await;
                // an incomplete check must not leave stale results pinned
                ctx.check_diags.insert(uri.clone(), Vec::new());
                Self::merge_and_publish(
                    ctx.pushing,
                    &ctx.client,
                    &ctx.check_diags,
                    analyzer_enabled,
                    cap,
                    uri,
                    snap_version,
                    &snap_text,
                    Self::open_docs_snapshot_of(&ctx.docs),
                    &ctx.catalog,
                    &ctx.catalog_origin,
                    false,
                    ctx.root.as_deref(),
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
                ctx.pushing,
                &ctx.client,
                &ctx.check_diags,
                analyzer_enabled,
                cap,
                uri,
                snap_version,
                &snap_text,
                Self::open_docs_snapshot_of(&ctx.docs),
                &ctx.catalog,
                &ctx.catalog_origin,
                false,
                ctx.root.as_deref(),
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
                        "kamailio-lsp: '{bin} -c' exceeded the output cap ({out_cap} bytes) on {check_path_str}; run discarded"
                    ),
                )
                .await;
            ctx.check_diags.insert(uri.clone(), Vec::new());
            Self::merge_and_publish(
                ctx.pushing,
                &ctx.client,
                &ctx.check_diags,
                analyzer_enabled,
                cap,
                uri,
                snap_version,
                &snap_text,
                Self::open_docs_snapshot_of(&ctx.docs),
                &ctx.catalog,
                &ctx.catalog_origin,
                false,
                ctx.root.as_deref(),
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
            .filter_map(|d| match &ctx.root {
                // a fragment shows the errors that name IT, at its own
                // lines; the root's and its siblings' belong to their
                // own buffers
                Some(_) => logic::fragment_check_diag(&check_path, &path, d),
                None => logic::remap_include_diag(&path, &doc_text, d),
            })
            .collect();
        if mapped.is_empty() && rc != 0 {
            // every positioned error pointed elsewhere (e.g. a nested
            // include): a failed check must never render as clean
            mapped.push(diag::Diag {
                file: path_str.clone(),
                line: 0,
                end_line: 0,
                col_start: 0,
                col_end: 1,
                severity: diag::Severity::Error,
                message: logic::check_failure_note(parsed.first(), rc),
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
            ctx.pushing,
            &ctx.client,
            &ctx.check_diags,
            analyzer_enabled,
            cap,
            uri,
            snap_version,
            &snap_text,
            Self::open_docs_snapshot_of(&ctx.docs),
            &ctx.catalog,
            &ctx.catalog_origin,
            false,
            ctx.root.as_deref(),
        )
        .await;
    }
}

/// Everything one spawned check task needs, snapshotted at spawn
/// time so the task owns its state outright.
struct CheckCtx {
    client: Client,
    docs: std::sync::Arc<DashMap<Uri, (i32, String)>>,
    catalog: std::sync::Arc<std::sync::RwLock<Vec<catalog::ModuleDoc>>>,
    catalog_origin: std::sync::Arc<std::sync::RwLock<catalog::CatalogOrigin>>,
    check_diags: std::sync::Arc<DashMap<Uri, Vec<Diagnostic>>>,
    check_gate: std::sync::Arc<tokio::sync::Mutex<()>>,
    bin: Option<String>,
    modules_path: Option<String>,
    check_timeout: std::time::Duration,
    analyzer_enabled: bool,
    cap: usize,
    uri: Uri,
    /// The config that must be handed to the checker when the open
    /// document is an included fragment: `kamailio -c` only accepts a
    /// whole program.  `None` when the document IS the program.
    root: Option<std::path::PathBuf>,
    /// Whether this server pushes diagnostics at all.
    pushing: bool,
    /// Invite a pulling client to re-ask once this check lands.
    refresh_after: bool,
    /// Suppress the empty publish a clean document gets in
    /// analyzer-only mode.  False for a re-check the user did not
    /// trigger by typing: a warning that is no longer true has to be
    /// taken off the screen.
    quiet_when_clean: bool,
}

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
        if let Some(b) = opts
            .get("inlayHintParameterNames")
            .and_then(|v| v.as_bool())
        {
            *self.inlay_parameter_names.write().unwrap() = b;
        }
        if let Some(b) = opts.get("inlayHintDefineValues").and_then(|v| v.as_bool()) {
            *self.inlay_define_values.write().unwrap() = b;
        }
        if let Some(b) = opts.get("codeLensReferences").and_then(|v| v.as_bool()) {
            *self.code_lens_refs.write().unwrap() = b;
        }
        self.apply_assistance(&opts);
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

        *self.version_in_hints.write().unwrap() = opts
            .get("versionInHints")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                std::env::var("KAMAILIO_LSP_VERSION_IN_HINTS")
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false)
            });

        *self.wanted_version.write().unwrap() = opts
            .get("kamailioVersion")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("KAMAILIO_LSP_VERSION").ok())
            .filter(|s| !s.is_empty());
        *self.watched_files_dynamic.write().unwrap() = p
            .capabilities
            .workspace
            .as_ref()
            .and_then(|w| w.did_change_watched_files.as_ref())
            .and_then(|d| d.dynamic_registration)
            .unwrap_or(false);
        // a client that pulls must not also be pushed to: it would
        // show every diagnostic twice
        *self.diagnostics_pulled.write().unwrap() = p
            .capabilities
            .text_document
            .as_ref()
            .and_then(|t| t.diagnostic.as_ref())
            .is_some();
        *self.diagnostics_refresh.write().unwrap() = p
            .capabilities
            .workspace
            .as_ref()
            .and_then(|w| w.diagnostics.as_ref())
            .and_then(|d| d.refresh_support)
            .unwrap_or(false);
        *self.workspace_roots.write().unwrap() = p
            .workspace_folders
            .iter()
            .flatten()
            .filter_map(|f| f.uri.to_file_path().map(|p| p.into_owned()))
            .collect();
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
            offset_encoding: None,
            server_info: Some(ServerInfo {
                name: "kamailio-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                // A server that does not SAY it handles workspace
                // folder changes is never told about them, and every
                // fragment in a folder added after startup stays
                // unrecognised until the window is reloaded.
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
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
                inlay_hint_provider: Some(OneOf::Left(true)),
                definition_provider: Some(OneOf::Left(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("kamailio-lsp".into()),
                        // a document's diagnostics depend on the files
                        // it includes, so an edit elsewhere can change
                        // them
                        inter_file_dependencies: true,
                        workspace_diagnostics: true,
                        work_done_progress_options: Default::default(),
                    },
                )),
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: Some(OneOf::Left(true)),
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
                call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
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
        let cached = self.harvest(true).await;
        self.register_watchers().await;
        // Core parameters, functions and pseudo-variables are the
        // LANGUAGE, not a module.  Without a configured wiki checkout
        // the harvest yields none of them, which left `debug`,
        // `children` and every other global uncompletable until the
        // user had cloned kamailio-wiki.  The vendored catalogue fills
        // that in; a real checkout always wins, because only that is
        // exact for the user's version.
        let builtin = {
            let core = self.core.read().unwrap();
            core.functions.is_empty() && core.params.is_empty() && core.pvars.is_empty()
        };
        if builtin {
            let mut core = catalog::builtin_core().core.clone();
            if *self.version_in_hints.read().unwrap() {
                // the core catalogue is one vendored artefact: it names
                // its OWN release, not the module release in use
                let note = catalog::version_note("core", &catalog::builtin_core().version);
                catalog::note_core(&mut core, &note);
            }
            *self.core.write().unwrap() = core;
        }
        // The same argument one level up: `is_method` is a textops
        // function, so a core-only fallback still left every module
        // call undocumented and `loadmodule "` offering nothing.  What
        // a module exports moves between releases, so a harvested tree
        // REPLACES this rather than merging — blending two versions
        // would be wrong in a way neither is alone.
        let builtin_mods = self.catalog.read().unwrap().is_empty();
        if builtin_mods {
            let wanted = self.wanted_version.read().unwrap().clone();
            // An unsupported release must not silently become the
            // newest: the user would then read warnings about a
            // release they did not ask for, with nothing saying so.
            let chosen = match wanted {
                Some(v) => match catalog::builtin_modules_at(&v) {
                    Some(b) => b,
                    None => {
                        self.client
                            .log_message(
                                MessageType::WARNING,
                                format!(
                                    "kamailio-lsp: no built-in catalogue for Kamailio {v} — \
                                     supported: {}; using {}",
                                    catalog::builtin_versioned().versions().join(", "),
                                    catalog::builtin_versioned().newest()
                                ),
                            )
                            .await;
                        catalog::builtin_modules().clone()
                    }
                },
                None => catalog::builtin_modules().clone(),
            };
            let mut modules = chosen.modules;
            if *self.version_in_hints.read().unwrap() {
                let note = catalog::version_note("module", &chosen.version);
                catalog::note_modules(&mut modules, &note);
            }
            *self.catalog.write().unwrap() = modules;
            *self.catalog_origin.write().unwrap() = catalog::CatalogOrigin::BuiltIn(chosen.version);
        }
        let n = self.catalog.read().unwrap().len();
        let c = self.core.read().unwrap().functions.len();
        let mut tag = if cached {
            ", cached".to_string()
        } else {
            String::new()
        };
        // the two catalogues have different provenance — core comes
        // from the wiki, modules from the source tree — so when both
        // are in use and the versions differ, say both rather than
        // letting one stand for the other
        let core_v = &catalog::builtin_core().version;
        let mods_v = &catalog::builtin_modules().version;
        match (builtin, builtin_mods) {
            (true, true) if core_v == mods_v => {
                tag.push_str(&format!(", core and module docs built in from {core_v}"))
            }
            (true, true) => tag.push_str(&format!(
                ", core docs built in from {core_v}, module docs built in from {mods_v}"
            )),
            (true, false) => tag.push_str(&format!(", core docs built in from {core_v}")),
            (false, true) => tag.push_str(&format!(", module docs built in from {mods_v}")),
            (false, false) => {}
        }
        // Tell the client what it is judging against, so the editor can
        // show it without waiting for something to go wrong.
        let origin = self.catalog_origin.read().unwrap().clone();
        self.client
            .send_notification::<CatalogueNotification>(CatalogueStatus {
                describe: origin.describe(),
                version: match &origin {
                    catalog::CatalogOrigin::BuiltIn(v) => Some(v.clone()),
                    catalog::CatalogOrigin::ConfiguredTree => None,
                },
            })
            .await;
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
        // an opened buffer outranks the disk copy the scan read
        self.invalidate_include_graph();
        self.spawn_check(&uri);
    }

    async fn did_change(&self, p: DidChangeTextDocumentParams) {
        let Some(change) = p.content_changes.into_iter().last() else {
            return;
        };
        let uri = p.text_document.uri;
        let version = p.text_document.version;
        let previous = self
            .docs
            .insert(uri.clone(), (version, change.text.clone()));
        // An edit that adds or removes an include directive changes
        // which file is a fragment of which; nothing else does, so the
        // graph survives ordinary typing.
        let targets = |text: &str| -> std::collections::HashSet<std::path::PathBuf> {
            uri.to_file_path()
                .map(|p| logic::resolved_includes(&p, text).into_iter().collect())
                .unwrap_or_default()
        };
        let before = previous.map(|(_, old)| targets(&old)).unwrap_or_default();
        let after = targets(&change.text);
        if before != after {
            self.invalidate_include_graph();
            // A file that just gained or lost an include is a file
            // whose analysis root just changed, and typing the
            // `include_file` line is the FIX for the warnings it was
            // showing.  Leaving them up until that buffer is next
            // touched makes the fix look like it did not work.
            let moved: Vec<std::path::PathBuf> =
                before.symmetric_difference(&after).cloned().collect();
            let stale: Vec<Uri> = self
                .docs
                .iter()
                .filter(|e| e.key() != &uri)
                .filter(|e| {
                    e.key()
                        .to_file_path()
                        .is_some_and(|p| moved.iter().any(|t| *t == *p))
                })
                .map(|e| e.key().clone())
                .collect();
            for u in stale {
                self.spawn_check_publishing(&u, false);
            }
        }
        // debounced analyzer pass: fast feedback between saves
        if !*self.analyzer_enabled.read().unwrap() || uri.scheme().as_str() != "file" {
            return;
        }
        let generation = {
            let mut e = self.change_gen.entry(uri.clone()).or_insert(0);
            *e += 1;
            *e
        };
        let gen_map = self.change_gen.clone();
        let pushing = !*self.diagnostics_pulled.read().unwrap();
        let check_map = self.check_diags.clone();
        let cat_arc = self.catalog.clone();
        let origin_arc = self.catalog_origin.clone();
        let client = self.client.clone();
        let open = self.open_docs_snapshot();
        let cap = *self.max_diagnostics.read().unwrap();
        let debounce = std::env::var("KAMAILIO_LSP_ANALYZER_DEBOUNCE_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300);
        let text = change.text;
        let root = self.analysis_root_of(&uri);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(debounce)).await;
            // superseded by a newer edit: let the newest task publish
            if gen_map.get(&uri).map(|g| *g) != Some(generation) {
                return;
            }
            Backend::merge_and_publish(
                pushing,
                &client,
                &check_map,
                true,
                cap,
                &uri,
                version,
                &text,
                open,
                &cat_arc,
                &origin_arc,
                false,
                root.as_deref(),
            )
            .await;
        });
    }

    async fn diagnostic(
        &self,
        p: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let uri = p.text_document.uri;
        let text = self.text_for(&uri);
        let diags = Self::merged_diags(
            &self.check_diags,
            *self.analyzer_enabled.read().unwrap(),
            *self.max_diagnostics.read().unwrap(),
            &uri,
            &text,
            self.open_docs_snapshot(),
            &self.catalog,
            &self.catalog_origin,
            self.analysis_root_of(&uri).as_deref(),
        );
        let id = Self::result_id(&diags);
        if p.previous_result_id.as_deref() == Some(id.as_str()) {
            return Ok(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Unchanged(RelatedUnchangedDocumentDiagnosticReport {
                    related_documents: None,
                    unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                        result_id: id,
                    },
                }),
            ));
        }
        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: Some(id),
                    items: diags,
                },
            }),
        ))
    }

    async fn workspace_diagnostic(
        &self,
        _: WorkspaceDiagnosticParams,
    ) -> Result<WorkspaceDiagnosticReportResult> {
        // Only ROOT configs are reported.  A config that another one
        // includes is a fragment, not a program: checking it on its own
        // would flag every route its parent defines as undefined and
        // every construct it continues as a syntax error.  Roots are
        // the files nothing else includes, and their closures already
        // cover the fragments.
        let (configs, truncated) = self.workspace_configs(500);
        let mut included: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();
        let mut texts: std::collections::HashMap<std::path::PathBuf, String> =
            std::collections::HashMap::new();
        for path in &configs {
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            let base = path.parent().unwrap_or(std::path::Path::new("."));
            for inc in analyze::includes(&text) {
                included.insert(base.join(&inc.name));
            }
            texts.insert(path.clone(), text);
        }
        if truncated {
            self.client
                .log_message(
                    MessageType::INFO,
                    "kamailio-lsp: workspace diagnostics stopped at 500 configs; \
                     the sweep is incomplete",
                )
                .await;
        }

        let analyzer_enabled = *self.analyzer_enabled.read().unwrap();
        let cap = *self.max_diagnostics.read().unwrap();
        let open = self.open_docs_snapshot();
        let mut items = Vec::new();
        for (path, text) in &texts {
            if included.contains(path) {
                continue;
            }
            let Some(uri) = Uri::from_file_path(path) else {
                continue;
            };
            // the open buffer wins over what is on disk
            let text = open.get(path).cloned().unwrap_or_else(|| text.clone());
            let diags = Self::merged_diags(
                &self.check_diags,
                analyzer_enabled,
                cap,
                &uri,
                &text,
                open.clone(),
                &self.catalog,
                &self.catalog_origin,
                // the sweep reports ROOTS only, so each is its own
                // analysis context by construction
                None,
            );
            items.push(WorkspaceDocumentDiagnosticReport::Full(
                WorkspaceFullDocumentDiagnosticReport {
                    uri,
                    version: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: Some(Self::result_id(&diags)),
                        items: diags,
                    },
                },
            ));
        }
        Ok(WorkspaceDiagnosticReportResult::Report(
            WorkspaceDiagnosticReport { items },
        ))
    }

    async fn did_change_workspace_folders(&self, p: DidChangeWorkspaceFoldersParams) {
        {
            let mut roots = self.workspace_roots.write().unwrap();
            for removed in &p.event.removed {
                if let Some(path) = removed.uri.to_file_path() {
                    roots.retain(|r| *r != *path);
                }
            }
            for added in &p.event.added {
                if let Some(path) = added.uri.to_file_path() {
                    let path = path.into_owned();
                    if !roots.contains(&path) {
                        roots.push(path);
                    }
                }
            }
        }
        // the graph is built from those folders and nothing else
        self.invalidate_include_graph();
        self.register_watchers().await;
    }

    async fn did_change_watched_files(&self, p: DidChangeWatchedFilesParams) {
        let src = self.src.read().unwrap().clone();
        let wiki = self.wiki.read().unwrap().clone();
        let under = |path: &std::path::Path, dir: &Option<String>| {
            dir.as_ref()
                .is_some_and(|d| path.starts_with(std::path::Path::new(d)))
        };
        let mut docs_changed = false;
        let mut changed: Vec<std::path::PathBuf> = Vec::new();
        for ev in &p.changes {
            let Some(path) = ev.uri.to_file_path() else {
                continue;
            };
            if under(&path, &src) || under(&path, &wiki) {
                docs_changed = true;
            } else {
                changed.push(path.into_owned());
            }
        }

        if docs_changed {
            // the fingerprint is content-aware, so this re-reads rather
            // than serving the stale cache entry
            self.harvest(false).await;
        }

        if changed.is_empty() {
            return;
        }
        // a config appearing, vanishing or gaining an include directive
        // changes which file is a fragment of which
        self.invalidate_include_graph();
        // an open document whose include closure contains a changed
        // file is answering from a stale read: re-check it
        let open: Vec<(Uri, String)> = self
            .docs
            .iter()
            .map(|e| (e.key().clone(), e.value().1.clone()))
            .collect();
        for (uri, text) in open {
            let closure = self.closure_for(&uri, &text);
            let own = uri.to_file_path().map(|p| p.into_owned());
            if closure
                .iter()
                .any(|(p, _)| changed.contains(p) && Some(p) != own.as_ref())
            {
                // not something the user typed: if the warning on
                // screen is no longer true, say so explicitly
                self.spawn_check_publishing(&uri, false);
            }
        }
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
        if let Some(b) = s.get("inlayHintParameterNames").and_then(|v| v.as_bool()) {
            *self.inlay_parameter_names.write().unwrap() = b;
        }
        if let Some(b) = s.get("inlayHintDefineValues").and_then(|v| v.as_bool()) {
            *self.inlay_define_values.write().unwrap() = b;
        }
        if let Some(b) = s.get("codeLensReferences").and_then(|v| v.as_bool()) {
            *self.code_lens_refs.write().unwrap() = b;
        }
        self.apply_assistance(s);
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
        let open_docs: Vec<(Uri, i32, String)> = self
            .docs
            .iter()
            .map(|e| (e.key().clone(), e.value().0, e.value().1.clone()))
            .collect();
        let pushing = !*self.diagnostics_pulled.read().unwrap();
        for (uri, version, text) in open_docs {
            Self::merge_and_publish(
                pushing,
                &self.client,
                &self.check_diags,
                analyzer_enabled,
                cap,
                &uri,
                version,
                &text,
                self.open_docs_snapshot(),
                &self.catalog,
                &self.catalog_origin,
                false,
                self.analysis_root_of(&uri).as_deref(),
            )
            .await;
        }
    }

    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        if let Some((_, task)) = self.check_tasks.remove(&p.text_document.uri) {
            task.abort();
        }
        self.doc_index.evict(&p.text_document.uri);
        self.docs.remove(&p.text_document.uri);
        self.check_diags.remove(&p.text_document.uri);
        self.change_gen.remove(&p.text_document.uri);
        // the buffer that outranked the disk copy is gone
        self.invalidate_include_graph();
    }

    async fn completion(&self, p: CompletionParams) -> Result<Option<CompletionResponse>> {
        if !*self.assistance.read().unwrap() {
            return Ok(None);
        }
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
                            logic::CompKind::Define => CompletionItemKind::CONSTANT,
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
        if !*self.assistance.read().unwrap() {
            return Ok(None);
        }
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
        // a preprocessor symbol is substituted before the parser sees
        // the name, so it wins over a same-named module symbol
        let files = self.closure_for(&uri, &text);
        if let Some(md) = logic::define_hover(&files, &word) {
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }),
                range: None,
            }));
        }
        let cat = self.catalog.read().unwrap();
        let core = self.core.read().unwrap();
        Ok(
            logic::hover_markdown_at(&cat, &core, &text, &word, pos.line, byte_col as u32).map(
                |md| Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: md,
                    }),
                    range: None,
                },
            ),
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
        // a preprocessor symbol resolves to its directive, including
        // when the cursor is on an `#!ifdef` operand rather than code
        if let Some(word) = analyze::word_at(&line_text, byte_col as usize) {
            let files = self.closure_for(&uri, &text);
            if let Some((path, d)) = logic::define_definition(&files, &word) {
                let target = if path.as_os_str().is_empty() {
                    Some(uri.clone())
                } else {
                    Uri::from_file_path(path)
                };
                if let Some(target) = target {
                    let ftext = files
                        .iter()
                        .find(|(p, _)| p == path)
                        .map(|(_, t)| t.clone())
                        .unwrap_or_default();
                    let dl = Self::doc_line(&ftext, d.line);
                    let c = analyze::byte_to_utf16(&dl, d.col as usize);
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: target,
                        range: Range {
                            start: Position::new(d.line, c),
                            end: Position::new(d.line, c + d.name.chars().count() as u32),
                        },
                    })));
                }
            }
        }
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
                && let Some(target) = Uri::from_file_path(path)
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
        let Some((_, text, idx)) = self.doc_with_index(&p.text_document.uri) else {
            return Ok(None);
        };
        #[allow(deprecated)]
        let syms: Vec<DocumentSymbol> = idx
            .blocks
            .iter()
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
        // preprocessor symbols belong in the outline too: they are
        // top-level bindings a reader navigates by, and in a large
        // config they are usually the first thing you look for
        let mut syms = syms;
        #[allow(deprecated)]
        for d in analyze::defines(&text) {
            let dl = Self::doc_line(&text, d.line);
            let start = Position::new(d.line, analyze::byte_to_utf16(&dl, d.col as usize));
            let end = Position::new(
                d.line,
                analyze::byte_to_utf16(&dl, d.col as usize + d.name.len()),
            );
            syms.push(DocumentSymbol {
                name: d.name,
                detail: Some(if d.value.is_empty() {
                    format!("#!{}", d.directive)
                } else {
                    format!("#!{} {}", d.directive, d.value)
                }),
                kind: SymbolKind::CONSTANT,
                tags: None,
                deprecated: None,
                range: Range { start, end },
                selection_range: Range { start, end },
                children: None,
            });
        }
        syms.sort_by_key(|s| (s.range.start.line, s.range.start.character));
        Ok(Some(DocumentSymbolResponse::Nested(syms)))
    }

    async fn references(&self, p: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let Some((_, text, idx)) = self.doc_with_index(&uri) else {
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
            // the root document's occurrences come from the memoized
            // index; included files are scanned directly
            let occs = if *path == root_path {
                logic::ns_occurrences_from(&idx.blocks, &idx.refs, &name, &ns)
            } else {
                logic::ns_occurrences(ftext, &name, &ns)
            };
            for (l, is_def) in occs {
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
                let target = Uri::from_file_path(&target)?;
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
            return Err(tower_lsp_server::jsonrpc::Error::invalid_params(format!(
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
                return Err(tower_lsp_server::jsonrpc::Error::invalid_params(
                    "event route names are defined by their module and cannot be renamed",
                ));
            }
            logic::RouteNs::Kind(k) => {
                // armed through module functions (t_on_failure, ...)
                // whose string arguments we cannot rewrite safely
                return Err(tower_lsp_server::jsonrpc::Error::invalid_params(format!(
                    "{k} names are referenced from module-function arguments; renaming here would not update those call sites"
                )));
            }
            logic::RouteNs::Main => {}
        }
        // rewrite every occurrence in the whole include closure
        let root_path = uri.to_file_path().unwrap_or_default();
        let files = self.closure_for(&uri, &text);
        let mut changes: std::collections::HashMap<Uri, Vec<TextEdit>> =
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

    async fn symbol(&self, p: WorkspaceSymbolParams) -> Result<Option<WorkspaceSymbolResponse>> {
        let query = p.query.to_lowercase();
        let mut seen: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();
        let mut out = Vec::new();
        // every open document plus its include closure, deduplicated
        let open: Vec<(Uri, String)> = self
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
                    match Uri::from_file_path(&path) {
                        Some(u) => u,
                        None => continue,
                    }
                };
                for b in analyze::route_blocks(&ftext) {
                    if b.name.is_empty() || !b.name.to_lowercase().contains(&query) {
                        continue;
                    }
                    let lt = Self::doc_line(&ftext, b.line);
                    let c = analyze::byte_to_utf16(&lt, b.col as usize);
                    // LSP 3.17 deprecates `SymbolInformation` here in
                    // favour of `WorkspaceSymbol`, and for this server
                    // that is a distinction without a difference:
                    // measured, the two encode BYTE-IDENTICALLY,
                    // because the useful half of `WorkspaceSymbol` is
                    // a location carrying only a URI, with the range
                    // fetched later by `workspaceSymbol/resolve`.  The
                    // ranges here are already in hand from the scan
                    // that found the symbols, so the lazy form would
                    // add a round-trip to deliver something already
                    // computed.  Revisit only if these become
                    // expensive to produce.
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
                        return Ok(Some(WorkspaceSymbolResponse::Flat(out)));
                    }
                }
            }
        }
        Ok(Some(WorkspaceSymbolResponse::Flat(out)))
    }

    async fn semantic_tokens_full(
        &self,
        p: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some((_, text, idx)) = self.doc_with_index(&p.text_document.uri) else {
            return Ok(None);
        };
        let data = logic::encode_spans(&text, &idx.spans)
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
        let Some((_, text, idx)) = self.doc_with_index(&p.text_document.uri) else {
            return Ok(None);
        };
        let r = p.range;
        let data = logic::encode_semantic_tokens_range_from(
            &text,
            &idx.spans,
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
        // Extract is offered for a real selection of whole lines, not
        // for a bare cursor or a word inside a line: it lifts LINES,
        // so offering it for a sub-line selection would move more than
        // the user highlighted.  A selection ending at column 0 of the
        // next line does not include that line, per the usual editor
        // convention.
        let sel_end = if p.range.end.character == 0 && p.range.end.line > p.range.start.line {
            p.range.end.line - 1
        } else {
            p.range.end.line
        };
        let start_line_text = Self::doc_line(&text, p.range.start.line);
        let covers_whole_lines = p.range.start.character == 0
            && (sel_end > p.range.start.line
                || p.range.end.character
                    >= analyze::byte_to_utf16(&start_line_text, start_line_text.len()));
        if covers_whole_lines
            && let Some(plan) = logic::extract_route(&text, p.range.start.line, sel_end)
        {
            let last = Self::doc_line(&text, plan.end_line);
            let mut edits = vec![
                // the selection becomes the call
                TextEdit {
                    range: Range {
                        start: Position::new(plan.start_line, 0),
                        end: Position::new(
                            plan.end_line,
                            analyze::byte_to_utf16(&last, last.len()),
                        ),
                    },
                    new_text: plan.call_line,
                },
                // and the new block lands after the enclosing one
                TextEdit {
                    range: Range {
                        start: Position::new(plan.insert_line, 0),
                        end: Position::new(plan.insert_line, 0),
                    },
                    new_text: plan.block,
                },
            ];
            edits.sort_by_key(|e| e.range.start.line);
            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), edits);
            out.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Extract into route[{}]", plan.name),
                kind: Some(CodeActionKind::REFACTOR_EXTRACT),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                ..Default::default()
            }));
        }

        // a second `loadmodule` for the same module is a parse error,
        // not untidiness: the real checker rejects the config
        let dups = logic::duplicate_loadmodules(&text);
        if !dups.is_empty() {
            let edits: Vec<TextEdit> = dups
                .iter()
                .map(|line| TextEdit {
                    // take the whole line including its newline
                    range: Range {
                        start: Position::new(*line, 0),
                        end: Position::new(line + 1, 0),
                    },
                    new_text: String::new(),
                })
                .collect();
            let mut changes = std::collections::HashMap::new();
            changes.insert(uri.clone(), edits);
            out.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!(
                    "Remove {} duplicate loadmodule line{}",
                    dups.len(),
                    if dups.len() == 1 { "" } else { "s" }
                ),
                kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    ..Default::default()
                }),
                ..Default::default()
            }));
        }

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
        let Some((_, text, idx)) = self.doc_with_index(&uri) else {
            return Ok(None);
        };
        let root_path = uri.to_file_path().unwrap_or_default();
        let files = self.closure_for(&uri, &text);
        let lenses = idx
            .blocks
            .iter()
            // only main-table (kind "route") names are route()-callable;
            // other kinds are armed via module functions we don't track,
            // so a count would mislead
            .filter(|b| b.kind == "route" && !b.name.is_empty())
            .map(|b| {
                // the root's refs come from the memoized index;
                // included files are scanned directly
                let n: usize = files
                    .iter()
                    .map(|(p, t)| {
                        if *p == root_path {
                            idx.refs.iter().filter(|r| r.name == b.name).count()
                        } else {
                            analyze::route_refs(t)
                                .into_iter()
                                .filter(|r| r.name == b.name)
                                .count()
                        }
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

    async fn inlay_hint(&self, p: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let mut hints: Vec<(logic::Hint, InlayHintKind)> = Vec::new();
        if *self.inlay_parameter_names.read().unwrap() {
            let cat = self.catalog.read().unwrap();
            let core = self.core.read().unwrap();
            hints.extend(
                logic::parameter_hints(&cat, &core, &text)
                    .into_iter()
                    .map(|h| (h, InlayHintKind::PARAMETER)),
            );
        }
        if *self.inlay_define_values.read().unwrap() {
            let files = self.closure_for(&p.text_document.uri, &text);
            hints.extend(
                logic::define_hints(&files, &text)
                    .into_iter()
                    .map(|h| (h, InlayHintKind::TYPE)),
            );
        }
        // the client asks for a viewport, so only pay for what is on
        // screen
        let out = hints
            .into_iter()
            .filter(|(h, _)| h.line >= p.range.start.line && h.line <= p.range.end.line)
            .map(|(h, kind)| {
                let lt = Self::doc_line(&text, h.line);
                let pad_left = kind == InlayHintKind::TYPE;
                InlayHint {
                    position: Position::new(h.line, analyze::byte_to_utf16(&lt, h.col as usize)),
                    label: InlayHintLabel::String(h.label),
                    kind: Some(kind),
                    text_edits: None,
                    tooltip: None,
                    padding_left: pad_left.then_some(true),
                    padding_right: (!pad_left).then_some(true),
                    data: None,
                }
            })
            .collect();
        Ok(Some(out))
    }

    async fn prepare_call_hierarchy(
        &self,
        p: CallHierarchyPrepareParams,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        let uri = p.text_document_position_params.text_document.uri;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let pos = p.text_document_position_params.position;
        // only the main table takes part: `route(NAME)` is the only
        // call form the server can see, so a failure_route or
        // event_route has no callers it could honestly report
        let Some((name, logic::RouteNs::Main)) =
            logic::route_symbol_ns_at(&text, pos.line, pos.character)
        else {
            return Ok(None);
        };
        let files = self.closure_for(&uri, &text);
        let Some((def_uri, def_text, block)) = Self::main_definition(&files, &name) else {
            // a call to a route that is defined nowhere: the analyzer
            // already flags it, and there is no item to anchor to
            return Ok(None);
        };
        Ok(Some(vec![Self::hierarchy_item(
            &def_uri, &block, &def_text,
        )]))
    }

    async fn incoming_calls(
        &self,
        p: CallHierarchyIncomingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        let Some(name) = Self::item_route(&p.item) else {
            return Ok(None);
        };
        let files = self.closure_for(&p.item.uri, &self.text_for(&p.item.uri));
        let mut out: Vec<CallHierarchyIncomingCall> = Vec::new();
        for (path, text) in &files {
            let Some(uri) = Uri::from_file_path(path) else {
                continue;
            };
            for (caller, call) in logic::call_edges(text) {
                if call.name != name {
                    continue;
                }
                // a call outside every block cannot be attributed to a
                // caller; the real parser rejects that config anyway
                let Some(caller) = caller else { continue };
                let range = Self::call_range(text, &call);
                let item = Self::hierarchy_item(&uri, &caller, text);
                match out.iter_mut().find(|c| {
                    c.from.uri == item.uri && c.from.selection_range == item.selection_range
                }) {
                    Some(existing) => existing.from_ranges.push(range),
                    None => out.push(CallHierarchyIncomingCall {
                        from: item,
                        from_ranges: vec![range],
                    }),
                }
            }
        }
        Ok(Some(out))
    }

    async fn outgoing_calls(
        &self,
        p: CallHierarchyOutgoingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        let Some(name) = Self::item_route(&p.item) else {
            return Ok(None);
        };
        let files = self.closure_for(&p.item.uri, &self.text_for(&p.item.uri));
        let Some((_, def_text, block)) = Self::main_definition(&files, &name) else {
            return Ok(None);
        };
        let mut out: Vec<CallHierarchyOutgoingCall> = Vec::new();
        for call in analyze::route_refs(&def_text)
            .into_iter()
            .filter(|c| c.line >= block.line && c.line <= block.end_line)
        {
            let range = Self::call_range(&def_text, &call);
            let to = match Self::main_definition(&files, &call.name) {
                Some((uri, text, b)) => Self::hierarchy_item(&uri, &b, &text),
                // a target defined nowhere is still an edge the reader
                // should see; say so rather than dropping it silently
                None => CallHierarchyItem {
                    name: format!("route[{}]", call.name),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    detail: Some("undefined".into()),
                    uri: p.item.uri.clone(),
                    range,
                    selection_range: range,
                    data: Some(serde_json::json!({ "route": call.name })),
                },
            };
            match out
                .iter_mut()
                .find(|c| c.to.uri == to.uri && c.to.selection_range == to.selection_range)
            {
                Some(existing) => existing.from_ranges.push(range),
                None => out.push(CallHierarchyOutgoingCall {
                    to,
                    from_ranges: vec![range],
                }),
            }
        }
        Ok(Some(out))
    }

    async fn formatting(&self, p: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let opts = crate::format::Options {
            insert_spaces: p.options.insert_spaces,
            tab_size: p.options.tab_size,
        };
        Ok(Some(Self::line_edits(
            &text,
            crate::format::format_lines(&text, &opts),
        )))
    }

    async fn range_formatting(
        &self,
        p: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let opts = crate::format::Options {
            insert_spaces: p.options.insert_spaces,
            tab_size: p.options.tab_size,
        };
        // the depth still comes from the whole document, so a range
        // lands where a full pass would have put it
        Ok(Some(Self::line_edits(
            &text,
            crate::format::format_range(&text, &opts, p.range.start.line, p.range.end.line),
        )))
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
