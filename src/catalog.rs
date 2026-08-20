//! Module and core documentation catalog.
//!
//! Module docs are harvested from the generated plain-text `README`
//! that ships in every `src/modules/<name>/` directory of a Kamailio
//! source tree. Core-language docs (parameters, functions,
//! pseudo-variables) come from a kamailio-wiki checkout
//! (`docs/cookbooks/<version>/{core,pseudovariables}.md`).

use std::path::{Path, PathBuf};

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

/// Parse one generated plain-text module `README`.
///
/// Structure (a lynx dump of the module docbook): chapter headings at
/// column 0 (`3. Parameters`, `4. Functions`), item headings at
/// column 0 (`3.1. fr_timer (integer)`, `4.1.  t_relay([host,
/// port])`), body paragraphs indented by three spaces. The table of
/// contents repeats every heading indented, so column 0 is the
/// anchor. Chapters restart numbering per guide (the developer guide
/// has its own `1. Available Functions`), so items are only collected
/// under a chapter literally titled `Parameters` / `Functions`
/// (optionally `Exported ...`).
pub fn parse_readme_txt(module: &str, txt: &str) -> Result<ModuleDoc, String> {
    if txt.contains('\0') {
        return Err("input contains NUL bytes".into());
    }
    if txt.trim().is_empty() {
        return Err("empty input".into());
    }
    static CHAPTER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static ITEM: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let chapter = CHAPTER.get_or_init(|| regex::Regex::new(r"^(\d+)\.\s+(\S.*)$").unwrap());
    let item = ITEM.get_or_init(|| regex::Regex::new(r"^(\d+)\.(\d+)\.\s+(\S.*)$").unwrap());

    #[derive(PartialEq, Clone, Copy)]
    enum Section {
        Params,
        Functions,
        Other,
    }
    let mut section = Section::Other;
    let mut out = ModuleDoc {
        name: module.to_string(),
        ..Default::default()
    };
    // (is_param, name, detail, doc-lines, doc-finished)
    let mut cur: Option<(bool, String, String, Vec<String>, bool)> = None;

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
        // item headings first: `3.1. name ...` would also match the
        // chapter regex (`3.` + ` 1. name`)
        if let Some(c) = item.captures(line) {
            flush(&mut cur, &mut out);
            if section == Section::Other {
                continue;
            }
            let heading = c[3].trim();
            match section {
                Section::Params => {
                    let (name, detail) = match heading.split_once(" (") {
                        Some((n, rest)) => (
                            n.trim().to_string(),
                            rest.trim_end_matches(')').trim().to_string(),
                        ),
                        None => (heading.to_string(), String::new()),
                    };
                    // dotted names (`tm.t_uac_start`) are RPC-style,
                    // never modparam targets — but chapter gating
                    // already excludes them; keep names word-shaped
                    cur = Some((true, name, detail, Vec::new(), false));
                }
                Section::Functions => {
                    let name = heading
                        .split('(')
                        .next()
                        .unwrap_or(heading)
                        .trim()
                        .to_string();
                    cur = Some((false, name, heading.to_string(), Vec::new(), false));
                }
                Section::Other => {}
            }
            continue;
        }
        if let Some(c) = chapter.captures(line) {
            flush(&mut cur, &mut out);
            section = match c[2].trim() {
                "Parameters" | "Exported Parameters" => Section::Params,
                "Functions" | "Exported Functions" => Section::Functions,
                _ => Section::Other,
            };
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
            && let Ok(m) = parse_readme_txt(&name, &txt)
        {
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

/// A cheap change-detector for a harvest: canonical paths plus the
/// modification times of the modules dir and the cookbook dir (module
/// or page additions/removals bump the directory mtimes).
pub fn tree_fingerprint(tree_root: &Path, wiki_root: Option<&Path>) -> String {
    use std::time::UNIX_EPOCH;
    let mtime = |p: &Path| -> u128 {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis())
            .unwrap_or(0)
    };
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let tree = canon(tree_root);
    let mods = modules_dir(&tree);
    let wiki_part = match wiki_root {
        Some(w) => {
            let w = canon(w);
            let book = cookbook_dir(&w).unwrap_or_else(|| w.clone());
            format!("{}|{}", w.display(), mtime(&book))
        }
        None => "none".to_string(),
    };
    let raw = format!("{}|{}|{}", tree.display(), mtime(&mods), wiki_part);
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
