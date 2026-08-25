//! Module and core documentation catalog.
//!
//! Module docs are harvested from the generated plain-text `README`
//! that ships in every `src/modules/<name>/` directory of a Kamailio
//! source tree. Core-language docs (parameters, functions,
//! pseudo-variables) come from a kamailio-wiki checkout
//! (`docs/cookbooks/<version>/{core,pseudovariables}.md`).

use std::path::{Path, PathBuf};

/// Where a module catalogue came from.
///
/// What a module exports moves between releases, so a diagnostic that
/// does not say which version it judged against cannot be acted on:
/// the reader cannot tell a typo from a parameter their build has and
/// this catalogue does not.
///
/// This is about the MODULE catalogue only. Core docs come from a
/// `kamailioWiki` checkout and are a separate question — the modparam
/// check never consults them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogOrigin {
    /// The vendored catalogue, at this upstream version.
    BuiltIn(String),
    /// A source tree the user pointed `kamailioSrc` at. It is exact
    /// for their build by construction, so it names no version.
    ConfiguredTree,
}

impl CatalogOrigin {
    /// How to name this catalogue inside a sentence.
    pub fn describe(&self) -> String {
        match self {
            Self::BuiltIn(v) => format!("Kamailio {v} (built in)"),
            Self::ConfiguredTree => "the configured source tree".to_string(),
        }
    }
}

/// One documented module symbol: a parameter or a function.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Item {
    /// Bare name (`fr_timer`, `t_relay`).
    pub name: String,
    /// Human detail: the param type (`integer`) or function signature.
    pub detail: String,
    /// First documentation paragraph, whitespace-collapsed.
    pub doc: String,
}

/// The harvested documentation of one Kamailio module.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModuleDoc {
    /// Module name (directory name under `src/modules/`).
    pub name: String,
    /// Exported parameters (`modparam` targets).
    pub params: Vec<Item>,
    /// Exported script functions.
    pub functions: Vec<Item>,
}

/// Harvested documentation is untrusted input rendered as Markdown in
/// editor popups: strip raw HTML and neutralize links whose scheme is
/// not http(s) (`command:`, `javascript:`, ...) — the label survives.
fn sanitize_doc(text: &str) -> String {
    static HTML: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static LINK: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let html = HTML.get_or_init(|| regex::Regex::new(r"</?[A-Za-z][^>]*>").unwrap());
    let link = LINK.get_or_init(|| {
        regex::Regex::new(r"\[([^\]]*)\]\(([A-Za-z][A-Za-z0-9+.-]*):[^)]*\)").unwrap()
    });
    let no_html = html.replace_all(text, "");
    link.replace_all(&no_html, |c: &regex::Captures| {
        let scheme = c[2].to_ascii_lowercase();
        if scheme == "http" || scheme == "https" {
            c[0].to_string()
        } else {
            c[1].to_string()
        }
    })
    .into_owned()
}

/// The type spellings a parameter heading uses.
///
/// Kamailio normally parenthesises the type — `fr_timer (integer)` —
/// but `ims_qos` writes `terminate_dialog_on_rx_failure integer` with
/// no parentheses at all, and the type then ended up inside the name,
/// where no `modparam` could ever match it.  Only a word that really
/// is a type may be split off: `crl` and `script_counter` are real
/// parameters documented with no type, and `slack url` is an upstream
/// typo for `slack_url`, not a type annotation.  `integer` is the only
/// spelling 6.1.4 writes bare; the rest are the spellings the same
/// corpus uses inside the parentheses.
const BARE_TYPE_WORDS: [&str; 8] = [
    "int", "integer", "string", "str", "float", "boolean", "bool", "flag",
];

/// `name` and `type` for a heading that annotates the type without
/// parentheses, or `None` when the title is not that shape.
fn split_bare_type(title: &str) -> Option<(&str, &str)> {
    let (name, ty) = title.rsplit_once(char::is_whitespace)?;
    let name = name.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let lowered = ty.to_ascii_lowercase();
    BARE_TYPE_WORDS
        .contains(&lowered.as_str())
        .then_some((name, ty))
}

/// Does this heading name an item, rather than a group of items?
///
/// `kazoo` groups its parameters — `4.1. amqp related` with the
/// parameters at `4.1.1.` — while `seas` documents `3.1.1. Return
/// value` under one of its functions.  The two are the same shape
/// upside down, and this is what tells them apart: an item carries a
/// signature or a type, a grouping or prose heading does not.
fn heading_is_item(title: &str) -> bool {
    title.contains('(') || split_bare_type(title).is_some()
}

/// Parse one generated plain-text module `README`.
///
/// Structure (a lynx dump of the module docbook): numbered headings
/// at column 0, body paragraphs indented.  The Parameters/Functions
/// chapter usually sits at the top level (`3. Parameters`, items
/// `3.1. fr_timer (integer)`), but some modules nest the whole admin
/// guide one level down (`2.3. Parameters`, items `2.3.1. brokers
/// (string)`) — so section matching is DEPTH-RELATIVE: items are the
/// headings exactly one component deeper that share the section's
/// number prefix.  Titles match case-insensitively (`Exported
/// parameters` is real), with an optional `Exported ` prefix.  The
/// table of contents repeats every heading indented, so column 0 is
/// the anchor; chapters restart numbering per guide (the developer
/// guide has its own `1. Available Functions`), and any heading at
/// the section's depth or shallower ends it.
pub fn parse_readme_txt(module: &str, txt: &str) -> Result<ModuleDoc, String> {
    if txt.contains('\0') {
        return Err("input contains NUL bytes".into());
    }
    if txt.trim().is_empty() {
        return Err("empty input".into());
    }
    static HEADING: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let heading = HEADING.get_or_init(|| regex::Regex::new(r"^((?:\d+\.)+)\s+(\S.*)$").unwrap());

    #[derive(PartialEq, Clone, Copy)]
    enum Section {
        Params,
        Functions,
        Other,
    }
    let mut section = Section::Other;
    // number prefix + component depth of the active section heading
    let mut sec_prefix = String::new();
    let mut sec_depth = 0usize;
    let mut out = ModuleDoc {
        name: module.to_string(),
        ..Default::default()
    };
    // (is_param, name, detail, doc-lines, doc-finished)
    let mut cur: Option<(bool, String, String, Vec<String>, bool)> = None;
    // the heading `cur` came from: its number prefix, its depth, and
    // whether it looked like an item.  A grouping heading is only
    // recognised as one when a deeper heading under it turns out to
    // be the real item, which is a fact from the NEXT heading.
    let mut cur_nums = String::new();
    let mut cur_depth = 0usize;
    let mut cur_is_item = false;

    let flush = |cur: &mut Option<(bool, String, String, Vec<String>, bool)>,
                 out: &mut ModuleDoc| {
        if let Some((is_param, name, detail, lines, _)) = cur.take() {
            if name.is_empty() {
                return;
            }
            let doc = sanitize_doc(
                &lines
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            let it = Item { name, detail, doc };
            if is_param {
                out.params.push(it);
            } else {
                out.functions.push(it);
            }
        }
    };

    for line in txt.lines() {
        // `Chapter N. <title>` is not a numbered heading, so a chapter
        // was invisible here — and `carrierroute` and `matrix` put
        // their database parameters in one of their own, items
        // restarting at `1.` at the top level.  Eleven parameters per
        // module went unharvested, and every configuration setting
        // one of them was warned about a parameter that exists.
        if let Some(rest) = line.strip_prefix("Chapter ")
            && rest.starts_with(|c: char| c.is_ascii_digit())
        {
            flush(&mut cur, &mut out);
            let title = rest.split_once(". ").map_or("", |(_, t)| t);
            if title.to_ascii_lowercase().contains("parameter") {
                section = Section::Params;
                sec_prefix = String::new();
                sec_depth = 0;
            } else {
                section = Section::Other;
            }
            continue;
        }
        if let Some(c) = heading.captures(line) {
            let nums = c[1].to_string();
            let depth = nums.bytes().filter(|b| *b == b'.').count();
            let title = c[2].trim();
            let lowered = title.to_ascii_lowercase();
            let bare = lowered.strip_prefix("exported ").unwrap_or(&lowered);
            if bare == "parameters" || bare == "functions" {
                flush(&mut cur, &mut out);
                section = if bare == "parameters" {
                    Section::Params
                } else {
                    Section::Functions
                };
                sec_prefix = nums;
                sec_depth = depth;
                continue;
            }
            // `cur` came from a heading that was NOT item-shaped and
            // this one, underneath it, is: the outer heading grouped
            // the items rather than being one, so it is dropped
            // unflushed and the item level moves down to here.  The
            // reverse nesting — an item-shaped heading with prose
            // under it, `seas`'s `Return value` — does not match, and
            // falls through to be ignored.
            let deepens = cur.is_some()
                && !cur_is_item
                && depth > cur_depth
                && nums.starts_with(&cur_nums)
                && heading_is_item(title);
            // `rtpengine` documents 85 parameters as `5.1.`…`5.86.`
            // and then drops back to `6.`, `7.`, … for the last nine.
            // A heading at the chapter's own depth normally ENDS the
            // chapter; what makes this safe is that a renumbered item
            // still carries its type and a chapter title
            // (`15. Functions`) does not — across the whole 6.1.4
            // tree the only nine headings this admits are rtpengine's
            // nine real parameters.
            let renumbered =
                section != Section::Other && depth <= sec_depth && heading_is_item(title);
            let in_section = section != Section::Other && nums.starts_with(&sec_prefix);
            let is_item = renumbered
                || (in_section
                    && (deepens
                        || depth == sec_depth + 1
                        || (cur.is_some() && depth == cur_depth && depth > sec_depth)));
            if is_item {
                if deepens {
                    cur = None; // a group heading is not an item
                } else {
                    flush(&mut cur, &mut out);
                }
                match section {
                    Section::Params => {
                        // `name (type)`, `name(type)` (presence writes
                        // `db_url(str)` unspaced) and `name type`
                        // (ims_qos writes no parentheses at all)
                        let (name, detail) = match title.split_once('(') {
                            Some((n, rest)) => (
                                n.trim().to_string(),
                                rest.trim_end_matches(')').trim().to_string(),
                            ),
                            None => match split_bare_type(title) {
                                Some((n, ty)) => (n.to_string(), ty.to_string()),
                                None => (title.to_string(), String::new()),
                            },
                        };
                        cur = Some((true, name, detail, Vec::new(), false));
                    }
                    Section::Functions => {
                        let name = title.split('(').next().unwrap_or(title).trim().to_string();
                        cur = Some((false, name, title.to_string(), Vec::new(), false));
                    }
                    Section::Other => {}
                }
                cur_nums = nums;
                cur_depth = depth;
                // a prose heading at the item level is only PROVISIONALLY
                // an item: if an item-shaped heading turns up under it,
                // it was a group all along and is retracted above
                cur_is_item = heading_is_item(title);
                continue;
            }
            flush(&mut cur, &mut out);
            if depth <= sec_depth {
                // a sibling or shallower chapter ends the section
                section = Section::Other;
                continue;
            }
            // deeper than an item: either a grouping heading whose
            // own items come next, or a sub-subsection of the item
            // just closed.  Remember it either way — which one it was
            // is decided by the heading that follows.
            if section != Section::Other && nums.starts_with(&sec_prefix) {
                cur_nums = nums;
                cur_depth = depth;
                cur_is_item = false;
            }
            continue;
        }
        if let Some((_, _, _, lines, finished)) = cur.as_mut() {
            // body paragraphs are indented; anything at column 0
            // (example blocks, rules) ends the doc summary
            if !line.starts_with(' ') && !line.trim().is_empty() {
                *finished = true;
                continue;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !lines.is_empty() {
                    *finished = true; // first paragraph complete
                }
            } else if !*finished {
                lines.push(trimmed.to_string());
            }
        }
    }
    flush(&mut cur, &mut out);
    Ok(out)
}

/// The modules directory of a tree: `src/modules` (a Kamailio source
/// checkout) or `modules` (being pointed at `src/` itself).
fn modules_dir(tree_root: &Path) -> PathBuf {
    let nested = tree_root.join("src").join("modules");
    if nested.is_dir() {
        nested
    } else {
        tree_root.join("modules")
    }
}

/// The outcome of reading one module's C parameter tables.
pub struct ModuleCParams {
    /// Every `modparam` name found, in declaration order.
    pub names: Vec<String>,
    /// Whether every table resolved fully. A table that splices in a
    /// macro we could not find leaves this false: the name set is then
    /// possibly short, so it must not be used to drop anything.
    pub complete: bool,
}

/// Skip ASCII whitespace in place.
fn skip_ws(b: &[u8], i: &mut usize) {
    while *i < b.len() && b[*i].is_ascii_whitespace() {
        *i += 1;
    }
}

/// Strip C comments so a commented-out table entry cannot be read as
/// an export. String and character literals are copied through: a
/// `//` inside a literal is text, not a comment.
fn strip_c_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            q @ (b'"' | b'\'') => {
                let start = i;
                i += 1;
                while i < b.len() {
                    if b[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if b[i] == q {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                out.extend_from_slice(&b[start..i.min(b.len())]);
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(b.len());
                out.push(b' ');
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Every `modparam` name declared by the `param_export_t` tables in
/// one C source file, in declaration order and de-duplicated.
///
/// This is the list `modparam()` is checked against when Kamailio
/// starts, so it decides which parameters exist; a module README only
/// says what they mean.
pub fn parse_param_export_tables(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut complete = true;
    scan_param_tables(
        src,
        &std::collections::BTreeMap::new(),
        &mut names,
        &mut complete,
    );
    names
}

/// Find every `param_export_t <ident>[] = { ... }` initialiser and
/// collect the names it declares.
fn scan_param_tables(
    src: &str,
    macros: &std::collections::BTreeMap<String, String>,
    out: &mut Vec<String>,
    complete: &mut bool,
) {
    const TY: &str = "param_export_t";
    let stripped = strip_c_comments(src);
    let b = stripped.as_bytes();
    let mut search = 0usize;

    while let Some(rel) = stripped[search..].find(TY) {
        let at = search + rel;
        search = at + TY.len();
        // a whole token, not the tail of some other identifier
        if at > 0 && (b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_') {
            continue;
        }
        // only `<ident>[] = {` opens a table; a prototype or a
        // `param_export_t *` parameter does not
        let mut i = search;
        skip_ws(b, &mut i);
        let id = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
            i += 1;
        }
        if i == id {
            continue;
        }
        let mut shaped = true;
        for expect in *b"[]=" {
            skip_ws(b, &mut i);
            if i >= b.len() || b[i] != expect {
                shaped = false;
                break;
            }
            i += 1;
        }
        if !shaped {
            continue;
        }
        skip_ws(b, &mut i);
        if i >= b.len() || b[i] != b'{' {
            continue;
        }
        search = collect_table_entries(&stripped, i, macros, out, complete, 8);
    }
}

/// Read one table initialiser, `start` at its opening brace, and
/// return the index just past its close.
///
/// Each entry opens a brace one level inside the table and its name is
/// the literal that opens it, so `{0, 0, 0}` terminators contribute
/// nothing and a literal deeper inside an entry's value is an argument
/// rather than a name.
fn collect_table_entries(
    src: &str,
    start: usize,
    macros: &std::collections::BTreeMap<String, String>,
    out: &mut Vec<String>,
    complete: &mut bool,
    budget: u8,
) -> usize {
    let b = src.as_bytes();
    let mut i = start;
    let mut depth = 0usize;

    while i < b.len() {
        match b[i] {
            b'{' => {
                depth += 1;
                if depth == 2 {
                    let mut j = i + 1;
                    skip_ws(b, &mut j);
                    if j < b.len() && b[j] == b'"' {
                        let s = j + 1;
                        let mut e = s;
                        while e < b.len() && b[e] != b'"' {
                            if b[e] == b'\\' {
                                e += 1;
                            }
                            e += 1;
                        }
                        let name = &src[s..e.min(src.len())];
                        if !name.is_empty() && !out.iter().any(|n| n == name) {
                            out.push(name.to_string());
                        }
                    }
                }
                i += 1;
            }
            b'}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                i += 1;
                if depth == 0 {
                    break;
                }
            }
            b'#' => {
                // A preprocessor directive brackets entries
                // conditionally — `tm` guards one behind
                // `USE_DNS_FAILOVER`, `tls` behind `KSR_SSL_ENGINE`. A
                // catalogue wants the union of both arms, so skip the
                // directive rather than reading `ifdef` as a macro
                // this parser cannot find.
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            c if depth == 1 && (c.is_ascii_alphabetic() || c == b'_') => {
                // A bare identifier between entries is a macro that
                // splices in more of them. `matrix` assembles its
                // WHOLE table this way — `matrix_DB_URL`,
                // `matrix_DB_TABLE`, `matrix_DB_COLS` from
                // `db_matrix.h` — so without expansion the module
                // reads as exporting nothing at all.
                let s = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                match macros.get(&src[s..i]) {
                    Some(body) if budget > 0 => {
                        let wrapped = format!("{{{body}}}");
                        collect_table_entries(&wrapped, 0, macros, out, complete, budget - 1);
                    }
                    _ => *complete = false,
                }
            }
            _ => i += 1,
        }
    }
    i
}

/// `#define <name> ...` bodies that look like table entries.
///
/// Only the ones containing a `{"` are kept: those are the ones a
/// parameter table can splice in.
fn collect_entry_macros(
    files: &[std::path::PathBuf],
) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let stripped = strip_c_comments(&text);
        let mut lines = stripped.lines();
        while let Some(line) = lines.next() {
            let Some(rest) = line.trim_start().strip_prefix("#define") else {
                continue;
            };
            let rest = rest.trim_start();
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            if end == 0 {
                continue;
            }
            let name = rest[..end].to_string();
            let mut body = rest[end..].to_string();
            while body.trim_end().ends_with('\\') {
                let Some(next) = lines.next() else { break };
                body.push(' ');
                body.push_str(next);
            }
            let body = body.replace('\\', " ");
            if body.contains("{\"") {
                out.insert(name, body);
            }
        }
    }
    out
}

/// Every C source under `root`, plus its headers when asked.
fn c_sources(root: &Path, headers: bool) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "c")
                || (headers && path.extension().is_some_and(|x| x == "h"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// The shared library tree a module's table can splice macros from.
/// Kamailio keeps it at `src/lib`; a tree rooted inside `src` has it
/// alongside `modules`.
fn lib_dir(tree_root: &Path) -> Option<std::path::PathBuf> {
    [tree_root.join("src").join("lib"), tree_root.join("lib")]
        .into_iter()
        .find(|candidate| candidate.is_dir())
}

/// Every `modparam` name a module exports, unioned across every
/// `param_export_t` table under its directory.
///
/// The union is deliberate. Over-collecting is permissive in both
/// directions this catalogue cares about — it drops fewer README
/// entries as phantoms and excuses fewer README examples — whereas
/// under-collecting warns at a configuration that is correct.
pub fn param_names_from_c(module_dir: &Path, tree_root: &Path) -> ModuleCParams {
    let mut macro_files = c_sources(module_dir, true);
    if let Some(lib) = lib_dir(tree_root) {
        macro_files.extend(c_sources(&lib, true));
    }
    let macros = collect_entry_macros(&macro_files);

    let mut names = Vec::new();
    let mut complete = true;
    for f in c_sources(module_dir, false) {
        let Ok(src) = std::fs::read_to_string(&f) else {
            continue;
        };
        scan_param_tables(&src, &macros, &mut names, &mut complete);
    }
    ModuleCParams { names, complete }
}

/// Reconcile a README harvest against the module's C parameter tables.
///
/// The C table decides which parameters exist; the README decides what
/// they mean. Two conditions hold a module back to its README harvest
/// untouched: no table at all — `textops` and the other function-only
/// modules export none — and a table this parser could not fully
/// resolve. Either way a parser regression degrades to the previous
/// behaviour rather than deleting real parameters.
fn reconcile_params_with_c(doc: &mut ModuleDoc, module_dir: &Path, tree_root: &Path) {
    let found = param_names_from_c(module_dir, tree_root);
    if found.names.is_empty() {
        return;
    }
    let module = doc.name.clone();
    if found.complete {
        // a heading that misnames a parameter put an entry in the
        // catalogue for something the module never exported
        doc.params.retain(|p| found.names.contains(&p.name));
    }
    for name in found.names {
        if doc.params.iter().any(|p| p.name == name) {
            continue;
        }
        doc.params.push(Item {
            name,
            detail: String::new(),
            doc: format!("Exported by `{module}`; not documented in the module README."),
        });
    }
}

/// Harvest every module's `README` under a Kamailio source tree.
pub fn harvest_tree(tree_root: &Path) -> Vec<ModuleDoc> {
    let mut out = Vec::new();
    let modules = modules_dir(tree_root);
    let Ok(entries) = std::fs::read_dir(&modules) else {
        return out;
    };
    for e in entries.flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let readme = e.path().join("README");
        if let Ok(txt) = std::fs::read_to_string(&readme)
            && let Ok(mut m) = parse_readme_txt(&name, &txt)
        {
            reconcile_params_with_c(&mut m, &e.path(), tree_root);
            out.push(m);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Shared heading-walker for the wiki cookbook markdown: yields
/// `(h2-section, h3-heading, first-paragraph)` triples, skipping
/// fenced code blocks. `h2`-level items are yielded with an empty
/// heading before their `###` children.
fn md_walk(md: &str) -> Result<Vec<(String, String, String)>, String> {
    if md.contains('\0') {
        return Err("input contains NUL bytes".into());
    }
    if md.trim().is_empty() {
        return Err("empty input".into());
    }
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut h2 = String::new();
    let mut cur: Option<(String, Vec<String>, bool)> = None;
    let mut in_fence = false;
    let flush = |h2: &str,
                 cur: &mut Option<(String, Vec<String>, bool)>,
                 out: &mut Vec<(String, String, String)>| {
        if let Some((name, lines, _)) = cur.take() {
            let doc = sanitize_doc(
                &lines
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            out.push((h2.to_string(), name, doc));
        }
    };
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(h) = line.strip_prefix("## ") {
            flush(&h2, &mut cur, &mut out);
            h2 = h.trim().to_string();
            // an H2 that is itself an item (pseudovariables.md mixes
            // levels): surface it with an empty h3 marker
            out.push((h2.clone(), String::new(), String::new()));
            continue;
        }
        if let Some(h) = line.strip_prefix("### ") {
            flush(&h2, &mut cur, &mut out);
            cur = Some((h.trim().to_string(), Vec::new(), false));
            continue;
        }
        if line.starts_with('#') {
            flush(&h2, &mut cur, &mut out);
            continue;
        }
        if let Some((_, lines, finished)) = cur.as_mut() {
            let t = line.trim();
            if t.is_empty() {
                if !lines.is_empty() {
                    *finished = true;
                }
            } else if !*finished {
                lines.push(t.to_string());
            }
        }
    }
    flush(&h2, &mut cur, &mut out);
    Ok(out)
}

/// Parse the wiki core cookbook (`core.md`): returns
/// `(parameters, functions)`. Parameters come from every `##
/// ... Parameters` section that documents cfg globals (Core, DNS,
/// TCP, TLS, SCTP, UDP, Blocklist, Real-Time); functions from `##
/// Core Functions`. Keywords, CLI parameters, and prose sections are
/// deliberately not symbols.
pub fn parse_core_cookbook_md(md: &str) -> Result<(Vec<Item>, Vec<Item>), String> {
    const PARAM_SECTIONS: &[&str] = &[
        "core parameters",
        "dns parameters",
        "tcp parameters",
        "tls parameters",
        "sctp parameters",
        "udp parameters",
        "blocklist parameters",
        "real-time parameters",
    ];
    let mut params = Vec::new();
    let mut functions = Vec::new();
    for (h2, h3, doc) in md_walk(md)? {
        if h3.is_empty() {
            continue;
        }
        let section = h2.to_ascii_lowercase();
        if PARAM_SECTIONS.contains(&section.as_str()) {
            let name = h3.split_whitespace().next().unwrap_or("").to_string();
            if name.is_empty() || name.contains('(') || name.starts_with('$') {
                continue;
            }
            params.push(Item {
                name,
                detail: h2.clone(),
                doc,
            });
        } else if section == "core functions" {
            let name = h3.split('(').next().unwrap_or("").trim().to_string();
            if name.is_empty() || name.contains(' ') {
                continue;
            }
            functions.push(Item {
                name,
                detail: h3.clone(),
                doc,
            });
        }
    }
    Ok((params, functions))
}

/// Parse the wiki pseudo-variables cookbook (`pseudovariables.md`):
/// every `##`/`###` heading naming a `$var` becomes an item. Names
/// are stored as `$` plus the leading word characters (`$avp`, not
/// `$avp(id)`); the full heading form survives in the detail.
pub fn parse_pvars_md(md: &str) -> Result<Vec<Item>, String> {
    static NAME: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let name_re = NAME.get_or_init(|| regex::Regex::new(r"^\$([A-Za-z0-9_]+)").unwrap());
    let mut out: Vec<Item> = Vec::new();
    let push = |heading: &str, doc: String, out: &mut Vec<Item>| {
        // wiki markdown escapes some specials in headings: `$\_s(...)`
        let unescaped = heading.replace('\\', "");
        let Some(c) = name_re.captures(&unescaped) else {
            return;
        };
        let name = format!("${}", &c[1]);
        if out.iter().any(|i| i.name == name) {
            return; // the list section and per-var sections overlap
        }
        let detail = match unescaped.split_once(" - ") {
            Some((_, desc)) => desc.trim().to_string(),
            None => unescaped.trim().to_string(),
        };
        out.push(Item { name, detail, doc });
    };
    for (h2, h3, doc) in md_walk(md)? {
        if h3.is_empty() {
            // an H2 that is itself a pvar section; its first paragraph
            // is not tracked by md_walk, so document from the heading
            if h2.starts_with('$') {
                push(&h2, String::new(), &mut out);
            }
            continue;
        }
        if h3.starts_with('$') {
            push(&h3, doc, &mut out);
        }
    }
    Ok(out)
}

/// Core-language documentation harvested from a kamailio-wiki
/// checkout.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CoreDocs {
    /// Core script functions (`core.md`, "Core Functions").
    pub functions: Vec<Item>,
    /// Global core parameters (`core.md`, the parameter sections).
    pub params: Vec<Item>,
    /// Pseudo-variables (`pseudovariables.md`), names include the `$`.
    pub pvars: Vec<Item>,
}

/// The cookbook directory inside a wiki checkout: either the given
/// path itself carries `core.md`, or the newest stable
/// `docs/cookbooks/<N.N.x>` is picked.
fn cookbook_dir(wiki_root: &Path) -> Option<PathBuf> {
    if wiki_root.join("core.md").is_file() {
        return Some(wiki_root.to_path_buf());
    }
    let books = wiki_root.join("docs").join("cookbooks");
    let mut best: Option<((u32, u32), PathBuf)> = None;
    for e in std::fs::read_dir(&books).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        // stable cookbooks are named N.N.x; ignore devel
        let Some(ver) = name.strip_suffix(".x") else {
            continue;
        };
        let Some((maj, min)) = ver.split_once('.') else {
            continue;
        };
        let (Ok(maj), Ok(min)) = (maj.parse::<u32>(), min.parse::<u32>()) else {
            continue;
        };
        if !e.path().join("core.md").is_file() {
            continue;
        }
        if best.as_ref().is_none_or(|(v, _)| (maj, min) > *v) {
            best = Some(((maj, min), e.path()));
        }
    }
    best.map(|(_, p)| p)
}

/// The vendored core catalogue: what the core language looks like in
/// the version this release pins.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuiltinCore {
    /// The Kamailio version the docs were harvested from.
    pub version: String,
    /// The harvested core docs.
    pub core: CoreDocs,
}

/// The built-in core catalogue, used when no wiki checkout is
/// configured.
///
/// Core parameters, functions and pseudo-variables are the LANGUAGE,
/// not a module: requiring a wiki checkout before `log_level`
/// completes makes the extension useless out of the box.  A wiki checkout the
/// user configures still wins, because only that is exact for their
/// build — so every built-in entry says which version it came from.
pub fn builtin_core() -> &'static BuiltinCore {
    static B: std::sync::OnceLock<BuiltinCore> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        let mut b: BuiltinCore = serde_json::from_str(include_str!("core_builtin.json"))
            .expect("the vendored core catalogue must parse");
        let note = format!(
            "\n\n*Built-in documentation from Kamailio {} — set `kamailioWiki` \
             to your own source tree for version-exact docs.*",
            b.version
        );
        for it in b
            .core
            .functions
            .iter_mut()
            .chain(b.core.params.iter_mut())
            .chain(b.core.pvars.iter_mut())
        {
            it.doc.push_str(&note);
        }
        b
    })
}

/// What one release changed from the release before it.
///
/// Adds and updates share `upserted` deliberately: applying either is
/// the same operation — replace the entry of that name, or insert it
/// if absent — and whether a given upsert is an addition or an edit is
/// a question about the previous release, not a fact worth storing
/// twice.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModuleDelta {
    /// The release this delta produces.
    pub version: String,
    /// Modules this release introduces, in full.
    pub modules_added: Vec<ModuleDoc>,
    /// Modules this release drops.
    pub modules_removed: Vec<String>,
    /// What changed inside modules present in both releases.
    pub changes: Vec<ModuleChange>,
}

/// How one surface — a module's parameters, or its functions —
/// changed between two releases.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SurfaceChange {
    /// Item-level edits, keyed by name.
    Edits {
        /// Names the release no longer exports.
        removed: Vec<String>,
        /// Entries added or altered, applied by name.
        upserted: Vec<Item>,
    },
    /// The whole list, replacing what was there.
    ///
    /// Names are not unique on every surface: thirteen surfaces per
    /// release document one name more than once — `avp`, `ims_auth`
    /// and `kazoo` among their functions, `matrix` among its
    /// parameters — and keying by name would silently merge those
    /// entries into one. Everything else stays item-level, which is
    /// what keeps a delta far below the cost of whole lists.
    Whole(Vec<Item>),
}

/// What one release changed inside one surviving module.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModuleChange {
    /// The module these changes apply to.
    pub module: String,
    /// How its parameters changed, if they did.
    pub params: Option<SurfaceChange>,
    /// How its functions changed, if they did.
    pub functions: Option<SurfaceChange>,
}

/// The vendored catalogue: one release in full, plus a forward delta
/// per later release.
///
/// Releases resemble each other far more than they differ, so
/// shipping each whole would be mostly duplicated bytes, and the
/// duplication would grow with every release added rather than
/// shrink.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VersionedModules {
    /// The oldest supported release, in full.
    pub base: BuiltinModules,
    /// Later releases, oldest first, each relative to the one before.
    pub deltas: Vec<ModuleDelta>,
}

/// Put a catalogue in canonical order: modules by name, and each
/// module's parameters and functions by name.
///
/// Order is not information here. Left alone it would be treated as
/// information by the delta: a README that merely reshuffled its
/// sections would produce a delta rewriting every entry it touched,
/// and a reconstructed release would differ from a fresh harvest over
/// nothing at all.
pub fn canonicalize(modules: &mut [ModuleDoc]) {
    modules.sort_by(|a, b| a.name.cmp(&b.name));
    for m in modules.iter_mut() {
        m.params.sort_by(|a, b| a.name.cmp(&b.name));
        m.functions.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

/// Whether a surface documents one name more than once.
fn has_duplicate_names(items: &[Item]) -> bool {
    let mut seen: Vec<&str> = Vec::with_capacity(items.len());
    for it in items {
        if seen.contains(&it.name.as_str()) {
            return true;
        }
        seen.push(&it.name);
    }
    false
}

/// Apply one item list over another by name: replace what is there,
/// append what is not.
fn upsert_items(into: &mut Vec<Item>, items: &[Item]) {
    for it in items {
        match into.iter_mut().find(|x| x.name == it.name) {
            Some(slot) => *slot = it.clone(),
            None => into.push(it.clone()),
        }
    }
}

/// How `new` differs from `old`, or `None` if it does not.
fn diff_surface(old: &[Item], new: &[Item]) -> Option<SurfaceChange> {
    if old == new {
        return None;
    }
    if has_duplicate_names(old) || has_duplicate_names(new) {
        return Some(SurfaceChange::Whole(new.to_vec()));
    }
    let removed = old
        .iter()
        .filter(|o| !new.iter().any(|n| n.name == o.name))
        .map(|o| o.name.clone())
        .collect();
    let upserted = new
        .iter()
        .filter(|n| old.iter().find(|o| o.name == n.name) != Some(n))
        .cloned()
        .collect();
    Some(SurfaceChange::Edits { removed, upserted })
}

/// Apply one surface change in place.
fn apply_surface(into: &mut Vec<Item>, change: &SurfaceChange) {
    match change {
        SurfaceChange::Whole(items) => *into = items.clone(),
        SurfaceChange::Edits { removed, upserted } => {
            into.retain(|i| !removed.contains(&i.name));
            upsert_items(into, upserted);
        }
    }
}

/// Compute what `newer` changed from `older`.
///
/// Both must already be in canonical order. Kept beside the type it
/// produces so the two cannot drift, and so a round-trip test can
/// state its property directly: applying this delta to `older` must
/// yield `newer`.
pub fn diff_catalogues(older: &[ModuleDoc], newer: &[ModuleDoc], version: &str) -> ModuleDelta {
    let mut delta = ModuleDelta {
        version: version.to_string(),
        ..Default::default()
    };
    for m in newer {
        if !older.iter().any(|o| o.name == m.name) {
            delta.modules_added.push(m.clone());
        }
    }
    for o in older {
        let Some(n) = newer.iter().find(|n| n.name == o.name) else {
            delta.modules_removed.push(o.name.clone());
            continue;
        };
        let change = ModuleChange {
            module: o.name.clone(),
            params: diff_surface(&o.params, &n.params),
            functions: diff_surface(&o.functions, &n.functions),
        };
        if change.params.is_some() || change.functions.is_some() {
            delta.changes.push(change);
        }
    }
    delta
}

impl VersionedModules {
    /// Every supported release, oldest first.
    pub fn versions(&self) -> Vec<&str> {
        std::iter::once(self.base.version.as_str())
            .chain(self.deltas.iter().map(|d| d.version.as_str()))
            .collect()
    }

    /// The newest supported release.
    pub fn newest(&self) -> &str {
        self.deltas
            .last()
            .map(|d| d.version.as_str())
            .unwrap_or(self.base.version.as_str())
    }

    /// The catalogue as it stood at `version`, or `None` if that
    /// release is not one of the supported ones.
    pub fn at(&self, version: &str) -> Option<Vec<ModuleDoc>> {
        if version == self.base.version {
            return Some(self.base.modules.clone());
        }
        if !self.deltas.iter().any(|d| d.version == version) {
            return None;
        }
        let mut modules = self.base.modules.clone();
        for delta in &self.deltas {
            modules.retain(|m| !delta.modules_removed.contains(&m.name));
            for change in &delta.changes {
                let Some(m) = modules.iter_mut().find(|m| m.name == change.module) else {
                    continue;
                };
                if let Some(c) = &change.params {
                    apply_surface(&mut m.params, c);
                }
                if let Some(c) = &change.functions {
                    apply_surface(&mut m.functions, c);
                }
            }
            modules.extend(delta.modules_added.iter().cloned());
            if delta.version == version {
                break;
            }
        }
        canonicalize(&mut modules);
        Some(modules)
    }

    /// Which supported releases export `param` from `module`.
    ///
    /// This is what turns "unknown parameter" into a version
    /// mismatch: a name absent from the release in use but present in
    /// another is almost never a typo.
    pub fn versions_with_param(&self, module: &str, param: &str) -> Vec<String> {
        self.versions()
            .into_iter()
            .filter(|v| {
                self.at(v).is_some_and(|mods| {
                    mods.iter()
                        .any(|m| m.name == module && m.params.iter().any(|p| p.name == param))
                })
            })
            .map(|v| v.to_string())
            .collect()
    }
}

/// The vendored module catalogue: every module the pinned release
/// documents, with its exported functions and parameters.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BuiltinModules {
    /// The Kamailio version the docs were harvested from.
    pub version: String,
    /// One entry per documented module.
    pub modules: Vec<ModuleDoc>,
}

/// The built-in module catalogue, used when no source tree is
/// configured.
///
/// `is_method` is a `textops` function, not core, so the core
/// catalogue alone still left every module call undocumented and
/// `loadmodule "` offering nothing at all.  What a module exports does
/// move between releases — which is exactly why a configured tree
/// REPLACES this wholesale rather than merging with it: two versions
/// blended together would be wrong in a way neither is on its own.
/// Every built-in entry says which version it came from.
pub fn builtin_modules() -> &'static BuiltinModules {
    static B: std::sync::OnceLock<BuiltinModules> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        let newest = builtin_versioned().newest();
        builtin_modules_at(newest).expect("the newest supported release must resolve")
    })
}

/// The vendored catalogue in full: the base release and every delta.
pub fn builtin_versioned() -> &'static VersionedModules {
    static V: std::sync::OnceLock<VersionedModules> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        serde_json::from_str(include_str!("modules_builtin.json"))
            .expect("the vendored module catalogue must parse")
    })
}

/// The catalogue as it stood at one supported release, `None` when
/// that release is not one of them.
///
/// Reconstructing costs applying the deltas, so callers that need it
/// repeatedly should hold on to the result rather than ask again.
pub fn builtin_modules_at(version: &str) -> Option<BuiltinModules> {
    let modules = builtin_versioned().at(version)?;
    let mut out = BuiltinModules {
        version: version.to_string(),
        modules,
    };
    let note = format!(
        "\n\n*Built-in documentation from Kamailio {} — set `kamailioSrc` \
         to your own source tree for version-exact docs.*",
        out.version
    );
    for m in out.modules.iter_mut() {
        for it in m.functions.iter_mut().chain(m.params.iter_mut()) {
            it.doc.push_str(&note);
        }
    }
    Some(out)
}

/// Harvest the core-language docs from a kamailio-wiki checkout;
/// missing or unparsable pages simply yield empty sections.
pub fn harvest_core(wiki_root: &Path) -> CoreDocs {
    let Some(dir) = cookbook_dir(wiki_root) else {
        return CoreDocs::default();
    };
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap_or_default();
    let (params, functions) = parse_core_cookbook_md(&read("core.md")).unwrap_or_default();
    let pvars = parse_pvars_md(&read("pseudovariables.md")).unwrap_or_default();
    CoreDocs {
        functions,
        params,
        pvars,
    }
}

/// Cache format/keying version: fold into the fingerprint so any
/// future change to what the harvest reads (or how it is keyed or
/// serialized) auto-invalidates every older cache entry.
pub const CACHE_SCHEMA_VERSION: u32 = 2;

/// `(size, mtime-in-millis)` of one file; `(0, 0)` when unreadable.
fn file_stamp(p: &Path) -> (u64, u128) {
    use std::time::UNIX_EPOCH;
    let Ok(md) = std::fs::metadata(p) else {
        return (0, 0);
    };
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    (md.len(), mtime)
}

/// A change-detector for a harvest: a manifest of every file the
/// harvest reads — each module's `README` (size + mtime; module
/// directories without one still contribute an entry, so additions
/// and removals register) and the wiki cookbook pages — plus the
/// canonical roots and [`CACHE_SCHEMA_VERSION`], hashed.  Editing a
/// harvested file's CONTENT invalidates even though no directory
/// mtime moves.
pub fn tree_fingerprint(tree_root: &Path, wiki_root: Option<&Path>) -> String {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let tree = canon(tree_root);
    let mods = modules_dir(&tree);
    // sorted (name, README size, README mtime) per module directory;
    // names are length-prefixed so hostile names (separators,
    // backslashes, quotes) cannot forge manifest boundaries
    let mut entries: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&mods) {
        for e in rd.flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let (size, mtime) = file_stamp(&e.path().join("README"));
            entries.push(format!("{}:{name}|{size}|{mtime}", name.len()));
        }
    }
    entries.sort();
    let wiki_part = match wiki_root {
        Some(w) => {
            let w = canon(w);
            let book = cookbook_dir(&w).unwrap_or_else(|| w.clone());
            let pages: Vec<String> = ["core.md", "pseudovariables.md"]
                .iter()
                .map(|f| {
                    let (size, mtime) = file_stamp(&book.join(f));
                    format!("{f}|{size}|{mtime}")
                })
                .collect();
            format!("{}|{}", w.display(), pages.join("|"))
        }
        None => "none".to_string(),
    };
    let raw = format!(
        "v{CACHE_SCHEMA_VERSION}|{}|{}|{wiki_part}",
        tree.display(),
        entries.join("|")
    );
    // stable, filesystem-safe name
    let mut h: u64 = 0xcbf29ce484222325;
    for b in raw.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheFile {
    modules: Vec<ModuleDoc>,
    core: CoreDocs,
}

/// Load a cached harvest for `(tree_root, wiki_root)`, if the cache
/// is present, parseable, and matches the current fingerprint.
pub fn load_cached(
    tree_root: &Path,
    wiki_root: Option<&Path>,
    cache_dir: &Path,
) -> Option<(Vec<ModuleDoc>, CoreDocs)> {
    let f = cache_dir.join(format!("{}.json", tree_fingerprint(tree_root, wiki_root)));
    let bytes = std::fs::read(f).ok()?;
    let c: CacheFile = serde_json::from_slice(&bytes).ok()?;
    Some((c.modules, c.core))
}

/// Persist a harvest under the current fingerprint.
pub fn save_cache(
    tree_root: &Path,
    wiki_root: Option<&Path>,
    cache_dir: &Path,
    modules: &[ModuleDoc],
    core: &CoreDocs,
) -> Result<(), String> {
    std::fs::create_dir_all(cache_dir).map_err(|e| e.to_string())?;
    let fp = tree_fingerprint(tree_root, wiki_root);
    let f = cache_dir.join(format!("{fp}.json"));
    let c = CacheFile {
        modules: modules.to_vec(),
        core: core.clone(),
    };
    let bytes = serde_json::to_vec(&c).map_err(|e| e.to_string())?;
    // atomic publish: concurrent servers may write the same file, and
    // readers must never see a torn cache — write-then-rename
    let tmp = cache_dir.join(format!(".{fp}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &f).map_err(|e| e.to_string())
}
