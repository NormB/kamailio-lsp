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
    /// A `#!define`-family preprocessor symbol.
    Define,
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
/// Control-flow keywords that are STATEMENTS, not calls: they are
/// written `exit;`, never `exit()`.
///
/// The core documentation lists several of them among its functions,
/// so without this they complete as tabstop snippets and typing
/// `exit` yields `exit()` — wrong in a way the parser then rejects.
/// It only showed up once core docs were available by default; with a
/// configured tree it had been wrong all along.
const STATEMENT_KEYWORDS: &[&str] = &[
    "if", "else", "switch", "case", "default", "while", "for", "exit", "drop", "return", "break",
];

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
    // preprocessor symbols are substituted textually, so they are
    // legal anywhere code is; the closure carries the ones an
    // included file defines
    for (_, t) in files {
        for d in analyze::defines(t) {
            out.push(Comp {
                label: d.name,
                detail: format!("#!{}", d.directive),
                doc: if d.value.is_empty() {
                    String::new()
                } else {
                    format!("`{}`", d.value)
                },
                kind: CompKind::Define,
            });
        }
    }
    for f in &core.functions {
        out.push(Comp {
            label: f.name.clone(),
            detail: f.detail.clone(),
            doc: f.doc.clone(),
            kind: if STATEMENT_KEYWORDS.contains(&f.name.as_str()) {
                CompKind::Keyword
            } else {
                CompKind::Function
            },
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

/// What extracting a selection into its own route would do.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractPlan {
    /// The generated route name, guaranteed not to collide.
    pub name: String,
    /// First selected line (0-based, inclusive).
    pub start_line: u32,
    /// Last selected line (0-based, inclusive).
    pub end_line: u32,
    /// The line that replaces the selection, at its indentation.
    pub call_line: String,
    /// 0-based line the new block is inserted before.
    pub insert_line: u32,
    /// The new `route[NAME] { ... }` block, newline-terminated.
    pub block: String,
}

/// Plan an extract-route refactoring for the lines `start..=end`, or
/// decline.
///
/// It declines far more than it accepts, and every refusal is a case
/// where accepting would change what the config does:
///
/// * outside a route block, or covering the block's own braces —
///   there is no body to lift, or lifting it would unbalance the file;
/// * unbalanced code braces inside the selection — the same;
/// * a `return` in the selection.  `return` leaves the route it is
///   written in; moved into a new route it returns to the CALLER, so
///   the statements after the extracted call would start running when
///   they did not before.  That is a behaviour change no editor should
///   make silently.
pub fn extract_route(doc: &str, start_line: u32, end_line: u32) -> Option<ExtractPlan> {
    let lines: Vec<&str> = doc.lines().collect();
    if start_line > end_line || end_line as usize >= lines.len() {
        return None;
    }
    let blocks = analyze::route_blocks(doc);
    let enclosing = enclosing_block(&blocks, start_line)?;
    // the selection must sit strictly inside the block's braces
    if start_line <= enclosing.line
        || end_line >= enclosing.end_line
        || !(start_line..=end_line).all(|l| {
            enclosing_block(&blocks, l).is_some_and(|b| {
                b.line == enclosing.line && b.name == enclosing.name && b.kind == enclosing.kind
            })
        })
    {
        return None;
    }

    let selected: Vec<&str> = lines[start_line as usize..=end_line as usize].to_vec();
    if selected.iter().all(|l| l.trim().is_empty()) {
        return None;
    }

    // brace balance and `return`, judged in code position only
    let class = crate::analyze::classify(doc);
    let mut off = doc
        .lines()
        .take(start_line as usize)
        .map(|l| l.len() + 1)
        .sum::<usize>();
    let mut depth = 0i32;
    for line in &selected {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let code = class.get(off + i) == Some(&crate::analyze::Class::Code);
            if code {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    _ => {}
                }
                if depth < 0 {
                    return None;
                }
                if bytes[i..].starts_with(b"return")
                    && (i == 0 || !crate::analyze::is_word_byte(bytes[i - 1]))
                    && bytes
                        .get(i + 6)
                        .is_none_or(|c| !crate::analyze::is_word_byte(*c))
                {
                    return None;
                }
            }
            i += 1;
        }
        off += line.len() + 1;
    }
    if depth != 0 {
        return None;
    }

    // a name nothing else uses
    let taken: std::collections::HashSet<String> = blocks.iter().map(|b| b.name.clone()).collect();
    let mut name = "EXTRACTED".to_string();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("EXTRACTED_{n}");
        n += 1;
    }

    let first = selected[0];
    let indent = &first[..first.len() - first.trim_start().len()];
    let mut block = format!("route[{name}] {{\n");
    for line in &selected {
        block.push_str(line);
        block.push('\n');
    }
    block.push_str("}\n");

    Some(ExtractPlan {
        call_line: format!("{indent}route({name});"),
        name,
        start_line,
        end_line,
        insert_line: enclosing.end_line + 1,
        block,
    })
}

/// The lines carrying a `loadmodule` that an earlier line already
/// loaded — every occurrence after the first, per module.
///
/// Loading a module twice is not a harmless tidy-up issue: the real
/// parser rejects the second load outright, so these lines are an
/// error the document can be repaired of.
pub fn duplicate_loadmodules(doc: &str) -> Vec<u32> {
    let mut seen = std::collections::HashSet::new();
    analyze::loaded_modules(doc)
        .into_iter()
        .filter(|m| !seen.insert(m.name.clone()))
        .map(|m| m.line)
        .collect()
}

/// One inlay hint: a label the editor draws at a position, without
/// changing the document.
#[derive(Debug, Clone, PartialEq)]
pub struct Hint {
    /// 0-based line.
    pub line: u32,
    /// 0-based byte column the label is drawn before.
    pub col: u32,
    /// The label as drawn, already punctuated.
    pub label: String,
}

/// The parameter name to draw for one signature parameter.
///
/// Signatures are written for humans — `[flags]`, `[outbound_proxy]`,
/// sometimes with a type in front — so the bracket markers and any
/// leading type are stripped down to the name itself.  A parameter
/// that reduces to nothing gets no hint at all rather than an empty
/// chip.
fn hint_name(param: &str) -> Option<String> {
    let p = param.trim().trim_matches(['[', ']']).trim();
    let p = p.split('=').next().unwrap_or(p).trim();
    let p = p.split_whitespace().last().unwrap_or(p);
    let p = p.trim_matches(['[', ']', '"', '\'']);
    (!p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_')).then(|| p.to_string())
}

/// The signature of `name`, preferring the modules the document
/// actually loads, then the core functions, then any other module —
/// the same order hover resolves in.
fn signature_of(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    name: &str,
) -> Option<String> {
    let loaded = loaded_names(doc);
    let sig = |m: &ModuleDoc| {
        m.functions
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.detail.clone())
    };
    catalog
        .iter()
        .filter(|m| loaded.contains(&m.name))
        .find_map(sig)
        .or_else(|| {
            core.functions
                .iter()
                .find(|f| f.name == name)
                .map(|f| f.detail.clone())
        })
        .or_else(|| {
            catalog
                .iter()
                .filter(|m| !loaded.contains(&m.name))
                .find_map(sig)
        })
}

/// Parameter-name hints for every documented call in `doc`.
///
/// Only calls the catalogue knows get hints, which is what keeps
/// keywords (`if`, `while`, `route`) out without special-casing them.
/// A call with more arguments than the signature documents is hinted
/// as far as the signature goes and no further — guessing past the
/// end would be inventing names.
pub fn parameter_hints(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
) -> Vec<Hint> {
    let mut out = Vec::new();
    for call in analyze::calls(doc) {
        if call.args.is_empty() {
            continue;
        }
        let Some(sig) = signature_of(catalog, core, doc, &call.name) else {
            continue;
        };
        let params = split_params(&sig);
        for (param, (line, col)) in params.iter().zip(call.args.iter()) {
            if let Some(name) = hint_name(param) {
                out.push(Hint {
                    line: *line,
                    col: *col,
                    label: format!("{name}:"),
                });
            }
        }
    }
    out
}

/// Value hints for preprocessor symbols: what each use expands to.
///
/// The definition site is skipped — `#!define PORT 5060` already says
/// `5060`, and repeating it there would be noise.  A define with no
/// value has nothing to show and is skipped everywhere.
pub fn define_hints(files: &[(std::path::PathBuf, String)], doc: &str) -> Vec<Hint> {
    let table: std::collections::HashMap<String, String> = files
        .iter()
        .flat_map(|(_, t)| analyze::defines(t))
        .filter(|d| !d.value.is_empty())
        .map(|d| (d.name, d.value))
        .collect();
    if table.is_empty() {
        return Vec::new();
    }
    let own: std::collections::HashSet<(u32, u32)> = analyze::defines(doc)
        .into_iter()
        .map(|d| (d.line, d.col))
        .collect();
    analyze::code_words(doc)
        .into_iter()
        .filter(|w| !own.contains(&(w.line, w.col)))
        .filter_map(|w| {
            table.get(&w.name).map(|v| Hint {
                line: w.line,
                col: w.col + w.name.len() as u32,
                label: format!("= {v}"),
            })
        })
        .collect()
}

/// Markdown hover for a preprocessor symbol, if `word` names one
/// anywhere in the closure.
///
/// This is checked before the module catalogue: the preprocessor
/// substitutes the name textually before the parser sees it, so a
/// define shadowing a module symbol really does win.
pub fn define_hover(files: &[(std::path::PathBuf, String)], word: &str) -> Option<String> {
    let d = files
        .iter()
        .flat_map(|(_, t)| analyze::defines(t))
        .find(|d| d.name == word)?;
    Some(if d.value.is_empty() {
        format!("**{}** — `#!{}` with no value", d.name, d.directive)
    } else {
        format!(
            "**{}** — `#!{}`\n\n```\n{}\n```",
            d.name, d.directive, d.value
        )
    })
}

/// The `#!define`-family directive binding `word` within the closure,
/// with the file it lives in.
pub fn define_definition<'a>(
    files: &'a [(std::path::PathBuf, String)],
    word: &str,
) -> Option<(&'a std::path::Path, analyze::Define)> {
    files.iter().find_map(|(p, t)| {
        analyze::defines(t)
            .into_iter()
            .find(|d| d.name == word)
            .map(|d| (p.as_path(), d))
    })
}

/// The modules `doc` loads, by name.
fn loaded_names(doc: &str) -> Vec<String> {
    analyze::loaded_modules(doc)
        .into_iter()
        .map(|m| m.name)
        .collect()
}

/// Hover text for `word` among the given modules, if one documents it.
fn hover_among<'a>(mods: impl Iterator<Item = &'a ModuleDoc>, word: &str) -> Option<String> {
    for m in mods {
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
    None
}

/// Markdown hover for `word`: loaded-module symbols win, then any
/// module's symbols, then module names themselves.
///
/// Core entries are not consulted here — [`hover_markdown_with_core`]
/// interleaves them, because a core global must outrank a same-named
/// parameter of a module the config never loads.
pub fn hover_markdown(catalog: &[ModuleDoc], doc: &str, word: &str) -> Option<String> {
    let loaded = loaded_names(doc);
    hover_among(catalog.iter().filter(|m| loaded.contains(&m.name)), word)
        .or_else(|| hover_among(catalog.iter().filter(|m| !loaded.contains(&m.name)), word))
        .or_else(|| hover_module_name(catalog, word))
}

/// Hover for a module name itself.
fn hover_module_name(catalog: &[ModuleDoc], word: &str) -> Option<String> {
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

/// The innermost route-family block whose extent covers `line`.
///
/// Route blocks do not nest in this language, so at most one can
/// match; the search is over `blocks` rather than the text so a
/// caller that already has them does not pay to re-scan.
pub fn enclosing_block(blocks: &[analyze::Block], line: u32) -> Option<&analyze::Block> {
    blocks.iter().find(|b| line >= b.line && line <= b.end_line)
}

/// Every `route(NAME)` call in `doc`, paired with the block it sits
/// in.  These are the edges of the route call graph.
///
/// The target of an edge is always a main-table `route[NAME]` block —
/// that is the route-namespace rule — but the source can be any
/// route-family block: a `failure_route` is not itself callable via
/// `route()`, yet it may call into the main table, and that is a real
/// edge.  A call outside every block carries `None`; the real parser
/// rejects such a config, but the analyzer must not assume it.
pub fn call_edges(doc: &str) -> Vec<(Option<analyze::Block>, Located)> {
    let blocks = analyze::route_blocks(doc);
    analyze::route_refs(doc)
        .into_iter()
        .map(|call| (enclosing_block(&blocks, call.line).cloned(), call))
        .collect()
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
/// (verified against the 6.0.1 and 6.1.4 binaries).  Rename must only ever
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

/// Expand a name through the `#!define` table.
///
/// The preprocessor substitutes textually and a definition may name
/// another definition, so the chain is followed — but only so far: a
/// circular pair (`#!define A B` / `#!define B A`) is legal text and
/// must terminate rather than spin.
fn expand_define(name: &str, defines: &std::collections::HashMap<String, String>) -> String {
    let mut cur = name.to_string();
    for _ in 0..8 {
        match defines.get(&cur) {
            Some(v) => cur = v.clone(),
            None => break,
        }
    }
    cur
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
    // `#!define` bindings from the whole closure: the preprocessor
    // expands a route target before the parser ever sees it, so a
    // route reached through an alias is not undefined
    let define_map: std::collections::HashMap<String, String> = files
        .iter()
        .flat_map(|(_, t)| analyze::defines(t))
        .map(|d| (d.name, d.value))
        .collect();
    let mut out = Vec::new();
    for r in analyze::route_refs(text) {
        // route(N) is a runtime rval expression (index/name dispatch):
        // purely numeric refs never warn
        if r.name.is_empty() || r.name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let expanded = expand_define(&r.name, &define_map);
        let via_define = expanded != r.name;
        // a bare `#!define NAME` expands to nothing: the result is
        // `route()`, which the real parser rejects — not ours to call
        if expanded.is_empty() || expanded.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if !defined.contains(expanded.as_str()) {
            out.push(AnalyzerDiag {
                line: r.line,
                col_start: r.col,
                col_end: r.col + r.name.len() as u32,
                message: if via_define {
                    format!(
                        "route '{}' expands to '{expanded}', which is not defined here or in included files",
                        r.name
                    )
                } else {
                    format!(
                        "route '{}' is not defined here or in included files",
                        r.name
                    )
                },
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

/// Catalog-pinned validation: flag `modparam("m", "p", ...)` where
/// the configured source tree documents module `m` but no parameter
/// `p`.  Version-exact by construction — the catalog IS the user's
/// pinned tree; unknown modules stay silent (the catalog may simply
/// not cover them).  Doc-derived, so kept separate from the
/// grammar-derived [`analyzer_diagnostics`] (and from the G1
/// differential gate, which proves the GRAMMAR model only).
pub fn catalog_diagnostics(catalog: &[ModuleDoc], text: &str) -> Vec<AnalyzerDiag> {
    if catalog.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for call in analyze::modparam_calls(text) {
        let Some(m) = catalog.iter().find(|m| m.name == call.module) else {
            continue;
        };
        if m.params.iter().any(|p| p.name == call.param) {
            continue;
        }
        out.push(AnalyzerDiag {
            line: call.line,
            col_start: call.col,
            col_end: call.col + call.param.len() as u32,
            message: format!(
                "parameter '{}' is not documented for module '{}' in the configured source tree",
                call.param, call.module
            ),
        });
    }
    out
}

/// Semantic-token categories the server emits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SemKind {
    /// A route name at a definition or call site (legend index 0).
    RouteName,
    /// A pseudo-variable (legend index 1).
    Pvar,
}

/// One semantic span, in BYTE columns on its line.
#[derive(Debug, Clone, PartialEq)]
pub struct SemSpan {
    /// 0-based line.
    pub line: u32,
    /// 0-based start byte column.
    pub col: u32,
    /// Byte length.
    pub len: u32,
    /// Category.
    pub kind: SemKind,
}

/// Semantic spans for a document: route names (definitions and call
/// sites) and pseudo-variables.  Pvars inside strings count — both
/// quote styles yield the same STRING token (cfg.lex STRING1/STRING2,
/// lines 1299-1316) and modules interpolate the value at fixup;
/// comments and `#!`/`!!` directives never contribute.
pub fn semantic_spans(text: &str) -> Vec<SemSpan> {
    let mut out = Vec::new();
    for b in analyze::route_blocks(text) {
        if !b.name.is_empty() {
            out.push(SemSpan {
                line: b.name_line,
                col: b.name_col,
                len: b.name.len() as u32,
                kind: SemKind::RouteName,
            });
        }
    }
    for r in analyze::route_refs(text) {
        out.push(SemSpan {
            line: r.line,
            col: r.col,
            len: r.name.len() as u32,
            kind: SemKind::RouteName,
        });
    }
    for (line, col, len) in analyze::pvars(text) {
        out.push(SemSpan {
            line,
            col,
            len,
            kind: SemKind::Pvar,
        });
    }
    out.sort_by_key(|s| (s.line, s.col));
    out.dedup_by_key(|s| (s.line, s.col));
    out
}

/// LSP semanticTokens/full data: delta-encoded quintuples with
/// UTF-16 columns and lengths.
pub fn encode_semantic_tokens(text: &str) -> Vec<u32> {
    encode_spans(text, &semantic_spans(text))
}

/// LSP semanticTokens/range data: the document's spans whose START
/// position falls inside `[start, end)` (positions as LSP UTF-16
/// (line, character) pairs), delta-encoded from a fresh origin — per
/// the LSP spec the first token's deltas are document-absolute, so a
/// range result stands alone.
pub fn encode_semantic_tokens_range(text: &str, start: (u32, u32), end: (u32, u32)) -> Vec<u32> {
    encode_semantic_tokens_range_from(text, &semantic_spans(text), start, end)
}

/// [`encode_semantic_tokens_range`] over prebuilt spans (the
/// memoized path).
pub fn encode_semantic_tokens_range_from(
    text: &str,
    spans: &[SemSpan],
    start: (u32, u32),
    end: (u32, u32),
) -> Vec<u32> {
    let lines: Vec<&str> = text.lines().collect();
    let spans: Vec<SemSpan> = spans
        .iter()
        .filter(|s| {
            let Some(line) = lines.get(s.line as usize) else {
                return false;
            };
            let col = analyze::byte_to_utf16(line, s.col as usize);
            (s.line, col) >= start && (s.line, col) < end
        })
        .cloned()
        .collect();
    encode_spans(text, &spans)
}

/// Delta-encode `spans` (sorted) against `text`.
pub fn encode_spans(text: &str, spans: &[SemSpan]) -> Vec<u32> {
    let lines: Vec<&str> = text.lines().collect();
    let mut data = Vec::new();
    let (mut prev_line, mut prev_start) = (0u32, 0u32);
    for s in spans {
        let Some(line) = lines.get(s.line as usize) else {
            continue;
        };
        let start = analyze::byte_to_utf16(line, s.col as usize);
        let end = analyze::byte_to_utf16(line, (s.col + s.len) as usize);
        let delta_line = s.line - prev_line;
        let delta_start = if delta_line == 0 {
            start - prev_start
        } else {
            start
        };
        data.extend_from_slice(&[
            delta_line,
            delta_start,
            end - start,
            match s.kind {
                SemKind::RouteName => 0,
                SemKind::Pvar => 1,
            },
            0,
        ]);
        prev_line = s.line;
        prev_start = start;
    }
    data
}

/// One quick-fix: a single text insertion with a title.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickFix {
    /// Human-readable action title.
    pub title: String,
    /// 0-based insertion line.
    pub line: u32,
    /// 0-based insertion column.
    pub col: u32,
    /// Text to insert.
    pub insert: String,
}

static_regex!(re_call_ident, r"([A-Za-z_][A-Za-z0-9_]*)\s*\(");
static_regex!(
    re_undefined_route,
    r"route '([A-Za-z0-9_.:-]+)' is not defined"
);

/// The function name called at/near byte column `col` on `line_text`:
/// kamailio's "unknown command, missing loadmodule?" names no
/// function and positions at the call's CLOSING paren (captured live,
/// 2026-08-20), so the identifier is recovered from the line — the
/// last `ident(` starting before the column, else the last on the
/// line.
fn called_function_at(line_text: &str, col: u32) -> Option<String> {
    let mut best: Option<String> = None;
    for c in re_call_ident().captures_iter(line_text) {
        let m = c.get(1).unwrap();
        if m.start() <= col as usize || best.is_none() {
            best = Some(m.as_str().to_string());
        }
    }
    best
}

/// Quick fixes for a diagnostic `message` positioned at 0-based
/// (`line`, `col`) in `doc`:
/// - `unknown command, missing loadmodule?` → load the module that
///   exports the function called at that position (one action per
///   exporting module, skipped when already loaded);
/// - `route 'x' is not defined` → append a `route[x]` stub (with an
///   `exit;` body — empty route bodies are not valid kamailio).
pub fn quick_fixes(
    catalog: &[ModuleDoc],
    doc: &str,
    message: &str,
    line: u32,
    col: u32,
) -> Vec<QuickFix> {
    let mut out = Vec::new();
    if message.contains("unknown command") {
        let line_text = doc.lines().nth(line as usize).unwrap_or("");
        if let Some(f) = called_function_at(line_text, col) {
            let loaded: Vec<String> = analyze::loaded_modules(doc)
                .into_iter()
                .map(|m| m.name)
                .collect();
            // insertion point: right after the LAST loadmodule line
            let insert_line = analyze::loaded_modules(doc)
                .into_iter()
                .map(|m| m.line + 1)
                .max()
                .unwrap_or(0);
            for m in catalog {
                if loaded.contains(&m.name) || !m.functions.iter().any(|x| x.name == f) {
                    continue;
                }
                out.push(QuickFix {
                    title: format!("Load module '{}' (exports {f})", m.name),
                    line: insert_line,
                    col: 0,
                    insert: format!("loadmodule \"{}.so\"\n", m.name),
                });
            }
        }
    }
    if let Some(c) = re_undefined_route().captures(message) {
        let name = &c[1];
        out.push(QuickFix {
            title: format!("Create route[{name}]"),
            line: doc.lines().count() as u32,
            col: 0,
            insert: format!("\nroute[{name}] {{\n\texit;\n}}\n"),
        });
    }
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
/// (Define > Function > Param > Route > Module > Keyword); ties keep
/// the first occurrence (loaded-module items precede core items).
///
/// A preprocessor symbol outranks everything: the substitution
/// happens before the parser sees the name, so if a define shadows a
/// module symbol the define is what the config actually means.
fn dedup_completions(items: Vec<Comp>) -> Vec<Comp> {
    fn rank(k: &CompKind) -> u8 {
        match k {
            CompKind::Define => 5,
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
                let kept = out[i].as_ref().unwrap();
                // On equal rank the documented one wins.  `exit` is
                // offered twice — once as a bare keyword, once from
                // the core docs — and both are keywords, so without
                // this the doc-less one survives purely by arriving
                // first and hover comes back empty.
                let better = rank(&c.kind) > rank(&kept.kind)
                    || (rank(&c.kind) == rank(&kept.kind)
                        && kept.doc.is_empty()
                        && !c.doc.is_empty());
                if better {
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

/// Where the hovered word sits, when the syntax decides the answer on
/// its own rather than leaving it to precedence.
///
/// A global `log_level=2` and `modparam("acc", "log_level",
/// 2)` are two different things that share a name, and the config says
/// which one you are looking at.  Resolving by precedence answers the
/// question the text already answered: with acc loaded, the
/// global assignment hovered as the module's parameter.
enum HoverSite {
    /// The parameter-name argument of `modparam("m", "p", ...)`: the
    /// module named in the call decides, not the loaded set.
    Modparam(String),
    /// A bare `name = value` statement — a core global.  Inside a
    /// route, assignment targets are pseudo-variables, which are not
    /// bare words, so this form is unambiguous.
    Global,
    /// Anywhere else: a call, an operand, a module name.
    Other,
}

fn hover_site(doc: &str, word: &str, line: u32, col: u32) -> HoverSite {
    if let Some(c) = analyze::modparam_calls(doc).into_iter().find(|c| {
        c.line == line && c.param == word && col >= c.col && col < c.col + c.param.len() as u32
    }) {
        return HoverSite::Modparam(c.module);
    }
    let Some(text) = doc.lines().nth(line as usize) else {
        return HoverSite::Other;
    };
    let lead = (text.len() - text.trim_start().len()) as u32;
    let rest = text.trim_start();
    let starts_here = col >= lead && col < lead + word.len() as u32;
    if starts_here && rest.starts_with(word) && rest[word.len()..].trim_start().starts_with('=') {
        return HoverSite::Global;
    }
    HoverSite::Other
}

/// [`hover_markdown_with_core`], with the syntax consulted first.
pub fn hover_markdown_at(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    word: &str,
    line: u32,
    col: u32,
) -> Option<String> {
    match hover_site(doc, word, line, col) {
        HoverSite::Modparam(module) => {
            // the module the call names, and only it
            if let Some(h) = hover_among(catalog.iter().filter(|m| m.name == module), word) {
                return Some(h);
            }
            // an undocumented module, or a parameter it does not
            // declare: fall through rather than answer nothing
        }
        HoverSite::Global => {
            if let Some(p) = core.params.iter().find(|p| p.name == word) {
                return Some(format!("**{}** — core parameter\n\n{}", p.name, p.doc));
            }
        }
        HoverSite::Other => {}
    }
    hover_markdown_with_core(catalog, core, doc, word)
}

/// [`hover_markdown`] plus core functions, core parameters, and
/// pseudo-variables (`word` may be given without the `$`).
pub fn hover_markdown_with_core(
    catalog: &[ModuleDoc],
    core: &crate::catalog::CoreDocs,
    doc: &str,
    word: &str,
) -> Option<String> {
    // A module the config never loads must not shadow the language.
    // Before the module catalogue shipped built in this was
    // unreachable without a source tree; with 254 modules always
    // present, hovering the core global `log_facility` resolved to
    // the `acc` modparam of the same name.
    let loaded = loaded_names(doc);
    if let Some(h) = hover_among(catalog.iter().filter(|m| loaded.contains(&m.name)), word) {
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
    // last: a module the config does not load, and module names
    hover_among(catalog.iter().filter(|m| !loaded.contains(&m.name)), word)
        .or_else(|| hover_module_name(catalog, word))
}

/// The per-document computations the hot request handlers share
/// (semantic tokens, code lens, document symbols, references,
/// folding): computed once per (document, version).
pub struct DocIndex {
    /// [`analyze::route_blocks`] of the document.
    pub blocks: Vec<analyze::Block>,
    /// [`analyze::route_refs`] of the document.
    pub refs: Vec<Located>,
    /// [`semantic_spans`] of the document.
    pub spans: Vec<SemSpan>,
}

/// Total [`DocIndex`] builds in this process — an instrumentation
/// seam for the memoization tests (and cheap enough to always keep).
static DOC_INDEX_BUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many [`DocIndex`] builds have run in this process.
pub fn doc_index_builds() -> usize {
    DOC_INDEX_BUILDS.load(std::sync::atomic::Ordering::Relaxed)
}

impl DocIndex {
    fn build(text: &str) -> Self {
        DOC_INDEX_BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            blocks: analyze::route_blocks(text),
            refs: analyze::route_refs(text),
            spans: semantic_spans(text),
        }
    }
}

/// A get-or-compute cache of [`DocIndex`] keyed by document, valid
/// for exactly one version at a time: asking for a different version
/// rebuilds and replaces (stale versions evict on the spot).  The
/// shard lock held across the entry guarantees concurrent requests
/// for the same (key, version) race safely to a single build.
pub struct DocCache<K: std::hash::Hash + Eq>(dashmap::DashMap<K, (i32, std::sync::Arc<DocIndex>)>);

impl<K: std::hash::Hash + Eq> Default for DocCache<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: std::hash::Hash + Eq> DocCache<K> {
    /// An empty cache.
    pub fn new() -> Self {
        Self(dashmap::DashMap::new())
    }

    /// The index for (`key`, `version`): reused when cached, built
    /// (and cached, replacing any other version) otherwise.
    pub fn get_or_index(&self, key: K, version: i32, text: &str) -> std::sync::Arc<DocIndex> {
        let mut entry = self
            .0
            .entry(key)
            .or_insert_with(|| (version, std::sync::Arc::new(DocIndex::build(text))));
        if entry.0 != version {
            *entry = (version, std::sync::Arc::new(DocIndex::build(text)));
        }
        entry.1.clone()
    }

    /// Drop a document's entry (e.g. on didClose).
    pub fn evict(&self, key: &K) {
        self.0.remove(key);
    }
}

/// [`ns_occurrences`] over a prebuilt index (the hot handlers'
/// memoized path); the free-standing function delegates here.
pub fn ns_occurrences_from(
    blocks: &[analyze::Block],
    refs: &[Located],
    name: &str,
    ns: &RouteNs,
) -> Vec<(Located, bool)> {
    if name.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(Located, bool)> = Vec::new();
    if *ns == RouteNs::Main {
        out.extend(
            refs.iter()
                .filter(|r| r.name == name)
                .cloned()
                .map(|r| (r, false)),
        );
    }
    let want_kind = match ns {
        RouteNs::Main => "route",
        RouteNs::Kind(k) => k.as_str(),
    };
    for b in blocks {
        if b.name == name && b.kind == want_kind {
            out.push((
                Located {
                    name: b.name.clone(),
                    line: b.name_line,
                    col: b.name_col,
                },
                true,
            ));
        }
    }
    out
}
