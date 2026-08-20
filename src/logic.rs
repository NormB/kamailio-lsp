//! Editor-feature logic, pure and testable.

use crate::analyze::{self, Located};
use crate::catalog::ModuleDoc;

/// What a completion item is, mapped to an LSP `CompletionItemKind`.
#[derive(Debug, Clone, PartialEq)]
pub enum CompKind {
    /// A loadable module name.
    Module,
    /// A `modparam` parameter of a module.
    Param,
    /// An exported script function.
    Function,
    /// A `route[...]` name.
    Route,
    /// A core language keyword.
    Keyword,
}

/// One completion candidate.
#[derive(Debug, Clone)]
pub struct Comp {
    /// The inserted/displayed label.
    pub label: String,
    /// Short detail (type, signature, or category).
    pub detail: String,
    /// Markdown documentation, empty if none.
    pub doc: String,
    /// Item category.
    pub kind: CompKind,
}

/// A small core-keyword seed; module functions dominate real usage.
const CORE_KEYWORDS: &[&str] = &[
    "if",
    "else",
    "switch",
    "case",
    "break",
    "default",
    "while",
    "exit",
    "drop",
    "return",
    "route",
    "forward",
    "setflag",
    "resetflag",
    "isflagset",
];

macro_rules! static_regex {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static regex::Regex {
            static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

static_regex!(re_modparam_first_arg, r#"modparamx?\s*\(\s*"[^"]*$"#);
static_regex!(re_loadmodule_arg, r#"loadmodulex?\s*\(?\s*"[^"]*$"#);
static_regex!(
    re_route_call_arg,
    r#"(?:^|[^A-Za-z0-9_])route\s*\(\s*["']?[A-Za-z0-9_.:-]*$"#
);

/// Is the cursor inside the name argument of a `route(...)` call?
fn route_call_context(line_prefix: &str) -> bool {
    re_route_call_arg().is_match(line_prefix)
}

/// Context-sensitive completion candidates for a cursor whose line
/// starts with `line_prefix`: modparam values/modules, loadmodule
/// targets, or (in code) loaded modules' functions, routes, keywords.
pub fn completions(catalog: &[ModuleDoc], doc: &str, line_prefix: &str) -> Vec<Comp> {
    let files = [(std::path::PathBuf::new(), doc.to_string())];
    complete_files(
        catalog,
        &crate::catalog::CoreDocs::default(),
        &files,
        line_prefix,
    )
}

/// The completion engine over an include closure (`files`: the
/// current document first, included files after).
fn complete_files(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    files: &[(std::path::PathBuf, String)],
    line_prefix: &str,
) -> Vec<Comp> {
    // "$" prefix → pseudo-variables, nothing else
    if pvar_tail(line_prefix).is_some() {
        return core
            .pvars
            .iter()
            .map(|v| Comp {
                label: v.name.clone(),
                detail: v.detail.clone(),
                doc: v.doc.clone(),
                kind: CompKind::Keyword,
            })
            .collect();
    }
    // modparam second argument → params of the named module
    if let Some(module) = analyze::modparam_context(line_prefix) {
        return catalog
            .iter()
            .filter(|m| m.name == module)
            .flat_map(|m| m.params.iter())
            .map(|p| Comp {
                label: p.name.clone(),
                detail: p.detail.clone(),
                doc: p.doc.clone(),
                kind: CompKind::Param,
            })
            .collect();
    }
    // modparam first argument → module names
    if re_modparam_first_arg().is_match(line_prefix) {
        return catalog
            .iter()
            .map(|m| Comp {
                label: m.name.clone(),
                detail: "module".into(),
                doc: String::new(),
                kind: CompKind::Module,
            })
            .collect();
    }
    // loadmodule argument → module .so names
    if re_loadmodule_arg().is_match(line_prefix) {
        return catalog
            .iter()
            .map(|m| Comp {
                label: format!("{}.so", m.name),
                detail: "module".into(),
                doc: String::new(),
                kind: CompKind::Module,
            })
            .collect();
    }

    let route_comp = |name: String| Comp {
        label: name,
        detail: "route".into(),
        doc: String::new(),
        kind: CompKind::Route,
    };
    let route_names = || {
        // only main-table names are route() targets
        route_blocks_multi(files)
            .into_iter()
            .filter(|(_, b)| b.kind == "route" && !b.name.is_empty())
            .map(|(_, b)| b.name)
    };
    // route(<cursor>) → route names only
    if route_call_context(line_prefix) {
        return route_names().map(route_comp).collect();
    }

    // plain code: functions of LOADED modules (anywhere in the
    // closure) + route names + keywords + core items
    let loaded = loaded_modules_multi(files);
    let mut out: Vec<Comp> = catalog
        .iter()
        .filter(|m| loaded.contains(&m.name))
        .flat_map(|m| m.functions.iter())
        .map(|f| Comp {
            label: f.name.clone(),
            detail: f.detail.clone(),
            doc: f.doc.clone(),
            kind: CompKind::Function,
        })
        .collect();
    out.extend(route_names().map(route_comp));
    for k in CORE_KEYWORDS {
        out.push(Comp {
            label: (*k).into(),
            detail: "keyword".into(),
            doc: String::new(),
            kind: CompKind::Keyword,
        });
    }
    for f in &core.functions {
        out.push(Comp {
            label: f.name.clone(),
            detail: f.detail.clone(),
            doc: f.doc.clone(),
            kind: CompKind::Function,
        });
    }
    for p in &core.params {
        out.push(Comp {
            label: p.name.clone(),
            detail: p.detail.clone(),
            doc: p.doc.clone(),
            kind: CompKind::Param,
        });
    }
    dedup_completions(out)
}

/// [`completions_with_core`] over an include closure.
pub fn completions_with_core_files(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    files: &[(std::path::PathBuf, String)],
    line_prefix: &str,
) -> Vec<Comp> {
    complete_files(catalog, core, files, line_prefix)
}

/// Markdown hover for `word`: loaded-module symbols win, then any
/// module's symbols, then module names themselves.
pub fn hover_markdown(catalog: &[ModuleDoc], doc: &str, word: &str) -> Option<String> {
    let loaded: Vec<String> = analyze::loaded_modules(doc)
        .into_iter()
        .map(|m| m.name)
        .collect();
    let ordered = catalog
        .iter()
        .filter(|m| loaded.contains(&m.name))
        .chain(catalog.iter().filter(|m| !loaded.contains(&m.name)));

    for m in ordered {
        if let Some(f) = m.functions.iter().find(|f| f.name == word) {
            return Some(format!(
                "```\n{}\n```\n*module {}*\n\n{}",
                f.detail, m.name, f.doc
            ));
        }
        if let Some(p) = m.params.iter().find(|p| p.name == word) {
            return Some(format!(
                "**{}** ({}) — modparam of `{}`\n\n{}",
                p.name, p.detail, m.name, p.doc
            ));
        }
    }
    catalog.iter().find(|m| m.name == word).map(|m| {
        format!(
            "**module {}** — {} params, {} functions",
            m.name,
            m.params.len(),
            m.functions.len()
        )
    })
}

/// Is byte position (line, col) inside the name span starting at `l`?
fn in_span(l: &Located, name: &str, line: u32, col: u32) -> bool {
    l.line == line && col >= l.col && col < l.col + name.len() as u32
}

/// Go-to-definition: a route name under the cursor resolves to its
/// `route[name]` block.  Matching is span-based, so dotted or
/// colon-carrying names like `to.b` or `htable:mod-init` resolve
/// whole.
pub fn definition_of(doc: &str, line: u32, col: u32) -> Option<Located> {
    let name = analyze::route_refs(doc)
        .into_iter()
        .find(|r| in_span(r, &r.name, line, col))?
        .name;
    // only main-table blocks are route() targets
    analyze::route_blocks(doc)
        .into_iter()
        .filter(|b| b.kind == "route")
        .map(|b| Located {
            name: b.name,
            line: b.line,
            col: b.col,
        })
        .find(|d| d.name == name)
}

/// The namespace a route-name symbol lives in.  Kamailio keeps one
/// route table per block kind: `route(NAME)` invokes only the main
/// table (`route[NAME]` blocks); `failure_route[NAME]` and friends
/// are armed through module functions and share nothing with the
/// main table (same-name cross-kind configs are legal).
#[derive(Debug, Clone, PartialEq)]
pub enum RouteNs {
    /// The main route table: `route[NAME]` defs + `route(NAME)` calls.
    Main,
    /// A per-kind table (`failure_route`, `event_route`, ...): just
    /// that kind's bracket names.
    Kind(String),
}

/// The route-name symbol whose span covers byte position (line, col)
/// — at a `route(name)` call site or at the NAME inside any named
/// block — together with its namespace.  Unnamed blocks have no
/// symbol.
pub fn route_symbol_ns_at(doc: &str, line: u32, col: u32) -> Option<(String, RouteNs)> {
    if let Some(r) = analyze::route_refs(doc)
        .into_iter()
        .find(|r| in_span(r, &r.name, line, col))
    {
        return Some((r.name, RouteNs::Main));
    }
    analyze::route_blocks(doc)
        .into_iter()
        .filter(|b| !b.name.is_empty())
        .find(|b| {
            let at = Located {
                name: b.name.clone(),
                line: b.name_line,
                col: b.name_col,
            };
            in_span(&at, &b.name, line, col)
        })
        .map(|b| {
            let ns = if b.kind == "route" {
                RouteNs::Main
            } else {
                RouteNs::Kind(b.kind.clone())
            };
            (b.name, ns)
        })
}

/// [`route_symbol_ns_at`], name only.
pub fn route_symbol_at(doc: &str, line: u32, col: u32) -> Option<String> {
    route_symbol_ns_at(doc, line, col).map(|(n, _)| n)
}

/// Every occurrence of `name` within its namespace in `doc`.  The
/// bool is `true` for definitions.  Main-namespace occurrences are
/// `route(NAME)` call sites plus `route[NAME]` definitions; per-kind
/// namespaces list only that kind's definition name spans (their call
/// sites are strings inside module functions like `t_on_failure`).
pub fn ns_occurrences(doc: &str, name: &str, ns: &RouteNs) -> Vec<(Located, bool)> {
    if name.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(Located, bool)> = Vec::new();
    if *ns == RouteNs::Main {
        out.extend(
            analyze::route_refs(doc)
                .into_iter()
                .filter(|r| r.name == name)
                .map(|r| (r, false)),
        );
    }
    let want_kind = match ns {
        RouteNs::Main => "route",
        RouteNs::Kind(k) => k.as_str(),
    };
    for b in analyze::route_blocks(doc) {
        if b.name == name && b.kind == want_kind {
            out.push((
                Located {
                    name: b.name,
                    line: b.name_line,
                    col: b.name_col,
                },
                true,
            ));
        }
    }
    out
}

/// Main-namespace occurrences of route `name` in `doc`.
pub fn route_occurrences(doc: &str, name: &str) -> Vec<(Located, bool)> {
    ns_occurrences(doc, name, &RouteNs::Main)
}

/// Is `s` legal as an UNQUOTED route name?  Kamailio's `route_name`
/// accepts a NUMBER, an ID (`[A-Za-z_][A-Za-z0-9_]*`, cfg.lex), or a
/// quoted string; dotted/dashed/colon names parse only when quoted
/// (verified against the 6.0.1 binary).  Rename must only ever
/// produce names that survive unquoted definitions, so it gates on
/// the ID charset.
pub fn valid_route_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// Remap one `kamailio -c` diagnostic onto the checked root file.
///
/// Kamailio attributes errors inside included files to the include
/// path AS WRITTEN — often relative to ITS cwd, which is not the
/// editor's.  A diagnostic for the checked file passes through; one
/// for a resolvable `include_file`/`import_file` target attaches to
/// the include directive in the root (context preserved in the
/// message); anything else is dropped (the caller falls back on
/// rc != 0).
pub fn remap_include_diag(
    checked: &std::path::Path,
    root_text: &str,
    d: &crate::diag::Diag,
) -> Option<crate::diag::Diag> {
    if diag_matches_file(&d.file, checked) {
        return Some(d.clone());
    }
    if d.file.is_empty() || d.file.contains('\0') {
        return None;
    }
    let norm = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let diag_path = std::path::Path::new(&d.file);
    let root_dir = checked.parent().unwrap_or_else(|| std::path::Path::new(""));
    // candidate spellings of the diag's file: as written; relative to
    // the checked file's directory; relative to our own cwd (the
    // subprocess inherited it)
    let mut cands: Vec<std::path::PathBuf> = vec![norm(diag_path)];
    if diag_path.is_relative() {
        cands.push(norm(&root_dir.join(diag_path)));
        if let Ok(cwd) = std::env::current_dir() {
            cands.push(norm(&cwd.join(diag_path)));
        }
    }
    for inc in analyze::includes(root_text) {
        let target = norm(&resolve_include(checked, &inc.name));
        if cands.contains(&target) {
            return Some(crate::diag::Diag {
                file: checked.display().to_string(),
                line: inc.line,
                end_line: inc.line,
                col_start: inc.col,
                // span the directive keyword; the range mapper clamps
                col_end: inc.col + 12,
                severity: d.severity,
                message: format!(
                    "in included file {}, line {}: {}",
                    inc.name,
                    d.line + 1,
                    d.message
                ),
            });
        }
    }
    None
}

/// Transitive include limits: a hostile config must stay cheap.
const INCLUDE_MAX_DEPTH: usize = 8;
const INCLUDE_MAX_FILES: usize = 64;

/// Resolve an `include_file`/`import_file` path: absolute paths as
/// written, relative paths against the INCLUDING file's directory.
fn resolve_include(from: &std::path::Path, inc: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(inc);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        from.parent()
            .unwrap_or_else(|| std::path::Path::new(""))
            .join(p)
    }
}

/// The root document plus every transitively included file, loaded via
/// `loader` (which lets callers prefer open editor buffers over disk).
/// Cycle-safe, depth- and count-capped; unloadable files are skipped.
pub fn include_closure(
    root_path: &std::path::Path,
    root_text: &str,
    loader: &dyn Fn(&std::path::Path) -> Option<String>,
) -> Vec<(std::path::PathBuf, String)> {
    let mut out: Vec<(std::path::PathBuf, String)> = Vec::new();
    let mut seen: std::collections::HashSet<std::path::PathBuf> = std::collections::HashSet::new();
    let mut queue: Vec<(std::path::PathBuf, String, usize)> =
        vec![(root_path.to_path_buf(), root_text.to_string(), 0)];
    seen.insert(root_path.to_path_buf());
    while let Some((path, text, depth)) = queue.pop() {
        if depth < INCLUDE_MAX_DEPTH {
            for inc in analyze::includes(&text) {
                if out.len() + queue.len() + 1 >= INCLUDE_MAX_FILES {
                    break;
                }
                let target = resolve_include(&path, &inc.name);
                if !seen.insert(target.clone()) {
                    continue;
                }
                if let Some(t) = loader(&target) {
                    queue.push((target, t, depth + 1));
                }
            }
        }
        out.push((path, text));
    }
    // root first, includes after, in a stable order
    out.sort_by(|a, b| {
        (a.0 != *root_path)
            .cmp(&(b.0 != *root_path))
            .then(a.0.cmp(&b.0))
    });
    out
}

/// Module names loaded anywhere in the closure, deduplicated.
pub fn loaded_modules_multi(files: &[(std::path::PathBuf, String)]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (_, text) in files {
        for m in analyze::loaded_modules(text) {
            if seen.insert(m.name.clone()) {
                out.push(m.name);
            }
        }
    }
    out
}

/// Route blocks (with kinds) across the closure, tagged with their
/// file.
pub fn route_blocks_multi(
    files: &[(std::path::PathBuf, String)],
) -> Vec<(std::path::PathBuf, analyze::Block)> {
    files
        .iter()
        .flat_map(|(p, text)| {
            analyze::route_blocks(text)
                .into_iter()
                .map(move |b| (p.clone(), b))
        })
        .collect()
}

/// Route definitions across the closure, tagged with their file.
pub fn route_defs_multi(
    files: &[(std::path::PathBuf, String)],
) -> Vec<(std::path::PathBuf, Located)> {
    files
        .iter()
        .flat_map(|(p, text)| {
            analyze::route_defs(text)
                .into_iter()
                .map(move |l| (p.clone(), l))
        })
        .collect()
}

/// A fast analyzer-side diagnostic (no `kamailio -c` involved).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzerDiag {
    /// 0-based line in the CURRENT file.
    pub line: u32,
    /// 0-based start byte column.
    pub col_start: u32,
    /// 0-based end byte column (exclusive).
    pub col_end: u32,
    /// Human-readable message.
    pub message: String,
}

/// Analyzer diagnostics for `text` (the file at `path`): `route(x)`
/// calls whose target is defined nowhere in the include closure, and
/// duplicate route definitions (every occurrence after the first is
/// flagged).  Only positions in the current file are reported.
pub fn analyzer_diagnostics(
    path: &std::path::Path,
    text: &str,
    loader: &dyn Fn(&std::path::Path) -> Option<String>,
) -> Vec<AnalyzerDiag> {
    let files = include_closure(path, text, loader);
    let blocks = route_blocks_multi(&files);
    // route(x) invokes only the MAIN table: route[x] blocks (kamailio
    // keeps per-kind tables; failure_route[x] does not satisfy it)
    let defined: std::collections::HashSet<&str> = blocks
        .iter()
        .filter(|(_, b)| b.kind == "route" && !b.name.is_empty())
        .map(|(_, b)| b.name.as_str())
        .collect();
    let mut out = Vec::new();
    for r in analyze::route_refs(text) {
        // route(N) is a runtime rval expression (index/name dispatch):
        // purely numeric refs never warn
        if r.name.is_empty() || r.name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if !defined.contains(r.name.as_str()) {
            out.push(AnalyzerDiag {
                line: r.line,
                col_start: r.col,
                col_end: r.col + r.name.len() as u32,
                message: format!(
                    "route '{}' is not defined here or in included files",
                    r.name
                ),
            });
        }
    }
    // duplicate definitions across the closure, PER KIND (cross-kind
    // same-name configs are legal); flag current-file ones
    let mut counts: std::collections::HashMap<(&str, &str), u32> = std::collections::HashMap::new();
    for (p, b) in &blocks {
        if b.name.is_empty() {
            continue;
        }
        let n = counts
            .entry((b.kind.as_str(), b.name.as_str()))
            .or_insert(0);
        *n += 1;
        if *n > 1 && p == path {
            out.push(AnalyzerDiag {
                line: b.line,
                col_start: b.col,
                col_end: b.col + 1,
                message: format!("{} '{}' is defined more than once", b.kind, b.name),
            });
        }
    }
    out.sort_by_key(|d| (d.line, d.col_start));
    out
}

/// Resolve the kamailio binary used for `-c` diagnostics.
///
/// Order: explicit initializationOption, then environment, then a
/// bare PATH lookup.  An EXPLICIT empty string (option or env)
/// disables diagnostics entirely — required so a user can opt out of
/// running `kamailio -c` (which dlopens the cfg's modules) on files
/// they do not trust.
pub fn resolve_bin(option: Option<&str>, env: Option<String>) -> Option<String> {
    match option {
        Some("") => None,
        Some(s) => Some(s.to_string()),
        None => match env {
            Some(e) if e.is_empty() => None,
            Some(e) => Some(e),
            None => Some("kamailio".to_string()),
        },
    }
}

/// Does a diagnostic reported for `diag_file` belong to the file we
/// checked?  Exact path match, or same basename (tolerates symlinked
/// spellings like /tmp vs /private/tmp); an empty `diag_file` is the
/// parser's global fallback and always attaches.
pub fn diag_matches_file(diag_file: &str, checked: &std::path::Path) -> bool {
    if diag_file.is_empty() {
        return true;
    }
    let diag_path = std::path::Path::new(diag_file);
    if diag_path == checked {
        return true;
    }
    // when both paths exist, canonical identity DECIDES: symlinked
    // spellings of one file match, and an included file that merely
    // shares the basename does not cross-attach
    if let (Ok(a), Ok(b)) = (
        std::fs::canonicalize(diag_path),
        std::fs::canonicalize(checked),
    ) {
        return a == b;
    }
    // otherwise (path gone, e.g. parser echoing a deleted temp file):
    // basename is the best remaining signal
    match (diag_path.file_name(), checked.file_name()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Resolve the `kamailio -c` run timeout: explicit option (ms) wins,
/// then `KAMAILIO_LSP_CHECK_TIMEOUT_MS`, then 10 seconds.  Clamped to
/// at least 1ms.
pub fn resolve_timeout(option_ms: Option<u64>, env: Option<String>) -> std::time::Duration {
    let ms = option_ms
        .or_else(|| env.and_then(|v| v.parse::<u64>().ok()))
        .unwrap_or(10_000);
    std::time::Duration::from_millis(ms.max(1))
}

/// [`completions`] plus core-language items: core functions and core
/// parameters in code position, and pseudo-variables when the cursor
/// follows a `$`.
pub fn completions_with_core(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    line_prefix: &str,
) -> Vec<Comp> {
    let files = [(std::path::PathBuf::new(), doc.to_string())];
    complete_files(catalog, core, &files, line_prefix)
}

/// If the cursor sits in a pseudo-variable context (`$` plus an
/// alphanumeric/`.`/`_` tail reaching the cursor), the byte length of
/// the `$`-prefixed token to replace.
pub fn pvar_tail(line_prefix: &str) -> Option<usize> {
    let (_, tail) = line_prefix.rsplit_once('$')?;
    tail.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        .then_some(1 + tail.len())
}

/// Collapse duplicate labels, keeping the most informative kind
/// (Function > Param > Route > Module > Keyword); ties keep the first
/// occurrence (loaded-module items precede core items).
fn dedup_completions(items: Vec<Comp>) -> Vec<Comp> {
    fn rank(k: &CompKind) -> u8 {
        match k {
            CompKind::Function => 4,
            CompKind::Param => 3,
            CompKind::Route => 2,
            CompKind::Module => 1,
            CompKind::Keyword => 0,
        }
    }
    let mut best: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut out: Vec<Option<Comp>> = Vec::with_capacity(items.len());
    for c in items {
        match best.get(&c.label) {
            Some(&i) => {
                if rank(&c.kind) > rank(&out[i].as_ref().unwrap().kind) {
                    out[i] = Some(c);
                }
            }
            None => {
                best.insert(c.label.clone(), out.len());
                out.push(Some(c));
            }
        }
    }
    out.into_iter().flatten().collect()
}

/// The signature under the cursor: the innermost UNCLOSED call in
/// `line_prefix` resolved against loaded-module functions first, then
/// any module's, then core functions.  Returns (signature, doc,
/// active-parameter index); commas inside strings do not advance the
/// index and a `#` comment ends the scan.
pub fn signature_at(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    line_prefix: &str,
) -> Option<(String, String, u32)> {
    let b = line_prefix.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut stack: Vec<(String, u32)> = Vec::new();
    let mut in_str = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'#' => break,
            b'(' => {
                let mut s = i;
                while s > 0 && (b[s - 1] as char).is_whitespace() {
                    s -= 1;
                }
                let e = s;
                while s > 0 && word(b[s - 1]) {
                    s -= 1;
                }
                let ident = std::str::from_utf8(&b[s..e]).unwrap_or("").to_string();
                stack.push((ident, 0));
            }
            b')' => {
                stack.pop();
            }
            b',' => {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let (name, commas) = stack.pop()?;
    if name.is_empty() {
        return None;
    }
    let loaded: Vec<String> = analyze::loaded_modules(doc)
        .into_iter()
        .map(|m| m.name)
        .collect();
    let ordered = catalog
        .iter()
        .filter(|m| loaded.contains(&m.name))
        .chain(catalog.iter().filter(|m| !loaded.contains(&m.name)));
    for m in ordered {
        if let Some(f) = m.functions.iter().find(|f| f.name == name) {
            return Some((f.detail.clone(), f.doc.clone(), commas));
        }
    }
    core.functions
        .iter()
        .find(|f| f.name == name)
        .map(|f| (f.detail.clone(), f.doc.clone(), commas))
}

/// Split a signature's parameter list on TOP-LEVEL commas only:
/// commas inside nested parens/brackets or inside quoted strings
/// (double or single) do not split.  Empty pieces are dropped.
pub fn split_params(sig: &str) -> Vec<String> {
    let Some(open) = sig.find('(') else {
        return Vec::new();
    };
    let Some(close) = sig.rfind(')') else {
        return Vec::new();
    };
    if open + 1 >= close {
        return Vec::new();
    }
    let inner = &sig[open + 1..close];
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str: Option<u8> = None;
    let mut start = 0usize;
    let b = inner.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match in_str {
            Some(q) => {
                if c == q {
                    in_str = None;
                }
            }
            None => match c {
                b'"' | b'\'' => in_str = Some(c),
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b',' if depth == 0 => {
                    out.push(inner[start..i].trim().to_string());
                    start = i + 1;
                }
                _ => {}
            },
        }
    }
    out.push(inner[start..].trim().to_string());
    out.retain(|p| !p.is_empty());
    out
}

/// [`hover_markdown`] plus core functions, core parameters, and
/// pseudo-variables (`word` may be given without the `$`).
pub fn hover_markdown_with_core(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    word: &str,
) -> Option<String> {
    if let Some(h) = hover_markdown(catalog, doc, word) {
        return Some(h);
    }
    if let Some(f) = core.functions.iter().find(|f| f.name == word) {
        return Some(format!(
            "```\n{}\n```\n*core function*\n\n{}",
            f.detail, f.doc
        ));
    }
    if let Some(p) = core.params.iter().find(|p| p.name == word) {
        return Some(format!("**{}** — core parameter\n\n{}", p.name, p.doc));
    }
    let dollar = format!("${word}");
    if let Some(v) = core
        .pvars
        .iter()
        .find(|v| v.name == word || v.name == dollar)
    {
        return Some(format!("**{}** — {}\n\n{}", v.name, v.detail, v.doc));
    }
    None
}
