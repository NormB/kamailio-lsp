//! Lightweight lexical analysis of kamailio.cfg documents.
//!
//! Not a grammar: a comment/string-aware scanner good enough for
//! completion, symbols and go-to-definition.  Full-fidelity semantic
//! validation is delegated to `kamailio -c` (see `diag`).

/// A named item found in a cfg document, with its position.
#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    /// Item name (module base name, route name, ...).
    pub name: String,
    /// 0-based line of the item's name.
    pub line: u32,
    /// 0-based start column of the name.
    pub col: u32,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Class {
    Code,
    Str,
    Comment,
    /// A `#!`/`!!` preprocessor directive, including backslash-
    /// continued lines (`#!define NAME value \`): directive text is
    /// never code, but it is not a comment either.
    Preproc,
}

/// Per-byte classification: code, string interior, comment, or
/// preprocessor directive.  Kamailio line comments are `#` and `//`
/// (cfg.lex COM_LINE); `#!` and `!!` start directives (PREP_START),
/// which extend across backslash-continued lines.
pub(crate) fn classify(text: &str) -> Vec<Class> {
    let b = text.as_bytes();
    let mut out = vec![Class::Code; b.len()];
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'#' | b'!' if i + 1 < b.len() && b[i + 1] == b'!' => {
                // directive: to EOL, following backslash continuations
                loop {
                    while i < b.len() && b[i] != b'\n' {
                        out[i] = Class::Preproc;
                        i += 1;
                    }
                    // continued? the byte before the newline is a '\'
                    if i < b.len() && i > 0 && b[i - 1] == b'\\' {
                        out[i] = Class::Preproc; // the newline itself
                        i += 1;
                        continue;
                    }
                    break;
                }
            }
            b'#' => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = Class::Comment;
                    i += 1;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = Class::Comment;
                    i += 1;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                out[i] = Class::Comment;
                out[i + 1] = Class::Comment;
                i += 2;
                while i < b.len() && !(b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/') {
                    out[i] = Class::Comment;
                    i += 1;
                }
                if i + 1 < b.len() {
                    out[i] = Class::Comment;
                    out[i + 1] = Class::Comment;
                    i += 2;
                }
            }
            b'"' => {
                // opening quote counts as code; interior as string
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    out[i] = Class::Str;
                    if b[i] == b'\\' && i + 1 < b.len() {
                        out[i + 1] = Class::Str;
                        i += 1;
                    }
                    i += 1;
                }
                if i < b.len() {
                    i += 1; // closing quote
                }
            }
            b'\'' => {
                // single-quoted string (cfg.lex STRING2): no escape
                // processing, may span lines
                i += 1;
                while i < b.len() && b[i] != b'\'' {
                    out[i] = Class::Str;
                    i += 1;
                }
                if i < b.len() {
                    i += 1; // closing tick
                }
            }
            _ => i += 1,
        }
    }
    out
}

/// Byte offset → (0-based line, 0-based col).
fn line_col(text: &str, offset: usize) -> (u32, u32) {
    let pre = &text.as_bytes()[..offset.min(text.len())];
    let line = pre.iter().filter(|&&c| c == b'\n').count() as u32;
    let col = offset
        - pre
            .iter()
            .rposition(|&c| c == b'\n')
            .map(|p| p + 1)
            .unwrap_or(0);
    (line, col as u32)
}

/// Word byte, for callers outside this module.
pub fn is_word_byte(c: u8) -> bool {
    is_word(c)
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

macro_rules! static_regex {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static regex::Regex {
            static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

static_regex!(re_loadmodule, r#"loadmodulex?\s*\(?\s*["']([^"'\n]+)["']"#);
static_regex!(
    re_route_def,
    r#"(?s)(request_route|reply_route|onreply_route|failure_route|branch_route|event_route|onsend_route|route)\s*(?:\[\s*["']?([A-Za-z0-9_.:-]+)["']?\s*\])?\s*\{"#
);
static_regex!(
    re_route_ref,
    r#"route\s*\(\s*["']?([A-Za-z0-9_.:-]+)["']?\s*[,)]"#
);
static_regex!(
    re_include,
    r#"(?:#!|!!)?(?:include_file|import_file)\s*["']([^"'\n]+)["']"#
);
static_regex!(
    re_modparam_ctx,
    r#"modparamx?\s*\(\s*"([^"\n]+)"\s*,\s*("[^"\n]*)?$"#
);

/// One `#!define`-family directive: the name it binds, the value it
/// binds it to, and where the NAME sits.
#[derive(Debug, Clone, PartialEq)]
pub struct Define {
    /// The bound name.
    pub name: String,
    /// The value, joined across backslash continuations; empty for a
    /// bare `#!define NAME` or an env-sourced form.
    pub value: String,
    /// The directive keyword as written (`define`, `substdef`, ...).
    pub directive: String,
    /// 0-based line of the NAME.
    pub line: u32,
    /// 0-based start column of the NAME.
    pub col: u32,
}

/// The directives that bind a bare identifier, per kamailio 6.1
/// `src/core/cfg.lex`.  Longest first: `def` is a prefix of `defexp`,
/// and matching in this order avoids binding the wrong keyword.
const DEFINING: &[&str] = &[
    "trydefenvs",
    "trydefenv",
    "trydefine",
    "redefine",
    "defexps",
    "defenvs",
    "trydef",
    "defexp",
    "defenv",
    "define",
    "redef",
    "def",
];

/// Every `#!define`-family directive in `text`.
///
/// Directives are read from the classifier's preprocessor runs, so a
/// `#!define` inside a string or a comment is not one, and a
/// backslash-continued directive is read whole — its continuation
/// lines are part of the value.
pub fn defines(text: &str) -> Vec<Define> {
    let class = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if class[i] != Class::Preproc {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && class[i] == Class::Preproc {
            i += 1;
        }
        if let Some(d) = parse_define(text, start, i) {
            out.push(d);
        }
    }
    out
}

/// Parse one preprocessor run (`text[start..end]`) as a definition.
///
/// Two shapes bind a name: the plain `#!define NAME value` family, and
/// `#!substdef "!NAME!value!g"`, whose first character after the quote
/// is the delimiter — whatever it happens to be.
fn parse_define(text: &str, start: usize, end: usize) -> Option<Define> {
    let run = text.get(start..end)?;
    // PREP_START is "#!" or "!!"
    let rest = run.strip_prefix("#!").or_else(|| run.strip_prefix("!!"))?;
    let after_kw = |k: &str| {
        rest.strip_prefix(k)
            .filter(|r| r.starts_with([' ', '\t']))
            .map(|r| (k.len(), r))
    };

    if let Some((kw_len, r)) = after_kw("substdefs").or_else(|| after_kw("substdef")) {
        let q = r.find('"')?;
        let body = &r[q + 1..];
        let mut chars = body.char_indices();
        let (_, delim) = chars.next()?;
        let first = body[delim.len_utf8()..].find(delim)? + delim.len_utf8();
        let name = &body[delim.len_utf8()..first];
        if name.is_empty() {
            return None;
        }
        let value_start = first + delim.len_utf8();
        let value = body[value_start..]
            .find(delim)
            .map(|e| &body[value_start..value_start + e])
            .unwrap_or("");
        let name_off = 2 + kw_len + q + 1 + delim.len_utf8();
        let (line, col) = line_col(text, start + name_off);
        return Some(Define {
            name: name.to_string(),
            value: value.to_string(),
            directive: if kw_len == 9 { "substdefs" } else { "substdef" }.to_string(),
            line,
            col,
        });
    }

    let (kw, r) = DEFINING
        .iter()
        .find_map(|k| after_kw(k).map(|(_, r)| (*k, r)))?;
    let name_off = r.len() - r.trim_start().len();
    let tail = &r[name_off..];
    let name_len = tail
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(tail.len());
    if name_len == 0 {
        return None;
    }
    let value = tail[name_len..]
        .replace('\\', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let (line, col) = line_col(text, start + 2 + kw.len() + name_off);
    Some(Define {
        name: tail[..name_len].to_string(),
        value,
        directive: kw.to_string(),
        line,
        col,
    })
}

/// Every identifier token in code or directive position.
///
/// Directive position counts because the preprocessor substitutes
/// textually: a name inside `#!ifdef` is a use of that name just as
/// much as one in a route body.
pub fn code_words(text: &str) -> Vec<Located> {
    let class = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !is_word(b[i]) || !matches!(class[i], Class::Code | Class::Preproc) {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && is_word(b[i]) {
            i += 1;
        }
        // a bare number is not an identifier
        if b[start].is_ascii_digit() {
            continue;
        }
        let (line, col) = line_col(text, start);
        out.push(Located {
            name: text[start..i].to_string(),
            line,
            col,
        });
    }
    out
}

/// A function call found in code position, with where each of its
/// top-level arguments begins.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// The called name as written.
    pub name: String,
    /// 0-based line of the name.
    pub line: u32,
    /// 0-based start column of the name.
    pub col: u32,
    /// 0-based (line, col) of the first real character of each
    /// top-level argument.  Empty for a call with no arguments.
    pub args: Vec<(u32, u32)>,
}

/// Every `name(...)` call in code position, with its argument
/// positions.
///
/// This is a scan, not a parse: it exists so inlay hints know where an
/// argument starts.  Commas inside strings or nested parentheses do
/// not split arguments, and a call written inside a string or a
/// comment is not a call.  Keywords like `if` and `route` come back
/// too — the catalogue lookup that consumes this simply will not know
/// them.
pub fn calls(text: &str) -> Vec<Call> {
    let class = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !is_word(b[i]) || class[i] != Class::Code {
            i += 1;
            continue;
        }
        // a name is only a name when nothing wordy precedes it
        if i > 0 && is_word(b[i - 1]) {
            while i < b.len() && is_word(b[i]) {
                i += 1;
            }
            continue;
        }
        let name_start = i;
        while i < b.len() && is_word(b[i]) {
            i += 1;
        }
        let name_end = i;
        let mut j = i;
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
            j += 1;
        }
        if j >= b.len() || b[j] != b'(' || class[j] != Class::Code {
            continue;
        }
        // walk the argument list, tracking nesting and quotes so a
        // comma inside either is not a separator
        let mut depth = 0i32;
        let mut quote: Option<u8> = None;
        let mut seg_start = j + 1;
        let mut args: Vec<(u32, u32)> = Vec::new();
        let mut k = j;
        let mut closed = false;
        while k < b.len() {
            let c = b[k];
            match quote {
                Some(q) => {
                    if c == b'\\' {
                        k += 2;
                        continue;
                    }
                    if c == q {
                        quote = None;
                    }
                }
                None => match c {
                    b'"' | b'\'' => quote = Some(c),
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            push_arg(text, seg_start, k, &mut args);
                            closed = true;
                            break;
                        }
                    }
                    b',' if depth == 1 => {
                        push_arg(text, seg_start, k, &mut args);
                        seg_start = k + 1;
                    }
                    _ => {}
                },
            }
            k += 1;
        }
        let (line, col) = line_col(text, name_start);
        out.push(Call {
            name: text[name_start..name_end].to_string(),
            line,
            col,
            args: if closed { args } else { Vec::new() },
        });
        i = name_end;
    }
    out
}

/// Record `text[from..to]` as an argument, unless it is blank.
fn push_arg(text: &str, from: usize, to: usize, out: &mut Vec<(u32, u32)>) {
    let seg = &text[from..to.min(text.len())];
    let Some(off) = seg.find(|c: char| !c.is_whitespace()) else {
        return;
    };
    out.push(line_col(text, from + off));
}

/// Every `loadmodule "x.so"` in code position, as bare module names.
pub fn loaded_modules(text: &str) -> Vec<Located> {
    let classes = classify(text);
    let mut out = Vec::new();
    for c in re_loadmodule().captures_iter(text) {
        let whole = c.get(0).unwrap();
        if classes.get(whole.start()) != Some(&Class::Code) {
            continue; // inside a comment, string, or directive
        }
        // reject matches that are a tail of a longer identifier
        if whole.start() > 0 && is_word(text.as_bytes()[whole.start() - 1]) {
            continue;
        }
        let path = c.get(1).unwrap();
        let base = path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".so");
        if base.is_empty() || base.contains('\0') {
            continue;
        }
        let (line, col) = line_col(text, whole.start());
        out.push(Located {
            name: base.to_string(),
            line,
            col,
        });
    }
    out
}

/// Every `include_file "x"` / `import_file "x"` — bare in code
/// position or `#!`/`!!`-prefixed as a directive (both are legal per
/// cfg.lex); `name` is the quoted path verbatim.
pub fn includes(text: &str) -> Vec<Located> {
    include_links(text)
        .into_iter()
        .map(|l| Located {
            name: l.path,
            line: l.dir_line,
            col: l.dir_col,
        })
        .collect()
}

/// One include directive with the exact span of its quoted path (for
/// documentLink) alongside the directive's own position.
#[derive(Debug, Clone, PartialEq)]
pub struct IncludeLink {
    /// The quoted path, verbatim.
    pub path: String,
    /// 0-based line of the PATH (inside the quotes).
    pub line: u32,
    /// 0-based start byte column of the path.
    pub col: u32,
    /// Byte length of the path.
    pub len: u32,
    /// 0-based line of the directive keyword.
    pub dir_line: u32,
    /// 0-based start column of the directive keyword.
    pub dir_col: u32,
}

/// [`includes`] with the path spans preserved.
pub fn include_links(text: &str) -> Vec<IncludeLink> {
    let classes = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    for c in re_include().captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = whole.start();
        let prefixed = whole.as_str().starts_with("#!") || whole.as_str().starts_with("!!");
        if prefixed {
            // must be the directive OPENER, not text inside another
            // directive's continued body
            if classes.get(start) != Some(&Class::Preproc)
                || (start > 0 && classes.get(start - 1) == Some(&Class::Preproc))
            {
                continue;
            }
        } else {
            if classes.get(start) != Some(&Class::Code) {
                continue;
            }
            // reject matches that are a tail of a longer identifier
            if start > 0 && is_word(b[start - 1]) {
                continue;
            }
        }
        let path = c.get(1).unwrap();
        if path.as_str().is_empty() || path.as_str().contains('\0') {
            continue;
        }
        let (dir_line, dir_col) = line_col(text, start);
        let (line, col) = line_col(text, path.start());
        out.push(IncludeLink {
            path: path.as_str().to_string(),
            line,
            col,
            len: (path.end() - path.start()) as u32,
            dir_line,
            dir_col,
        });
    }
    out
}

/// One `modparam("module", "param", ...)` call site.
#[derive(Debug, Clone, PartialEq)]
pub struct ModparamCall {
    /// First argument: the module name.
    pub module: String,
    /// Second argument: the parameter name.
    pub param: String,
    /// 0-based line of the PARAM name.
    pub line: u32,
    /// 0-based start column of the PARAM name (inside its quotes).
    pub col: u32,
}

static_regex!(
    re_modparam_call,
    r#"modparamx?\s*\(\s*["']([^"'\n]+)["']\s*,\s*["']([^"'\n]+)["']"#
);

/// Every `modparam("m", "p", ...)` / `modparamx(...)` in code
/// position, with the position of the parameter name.
pub fn modparam_calls(text: &str) -> Vec<ModparamCall> {
    let classes = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    for c in re_modparam_call().captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = whole.start();
        if classes.get(start) != Some(&Class::Code) {
            continue;
        }
        if start > 0 && is_word(b[start - 1]) {
            continue;
        }
        let (module, param) = (&c[1], &c[2]);
        if module.contains('\0') || param.contains('\0') {
            continue;
        }
        let pm = c.get(2).unwrap();
        let (line, col) = line_col(text, pm.start());
        out.push(ModparamCall {
            module: module.to_string(),
            param: param.to_string(),
            line,
            col,
        });
    }
    out
}

static_regex!(
    re_pvar_tok,
    r"\$[A-Za-z][A-Za-z0-9_.]*(?:\([A-Za-z0-9_.:>=-]*\))?"
);

/// Every pseudo-variable occurrence as (line, col, byte length).
/// Pvars inside strings COUNT — both quote styles produce the same
/// STRING token (cfg.lex STRING1/STRING2) and module fixups
/// interpolate the value afterwards; comments and `#!` directives
/// never contribute.
pub fn pvars(text: &str) -> Vec<(u32, u32, u32)> {
    let classes = classify(text);
    let mut out = Vec::new();
    for m in re_pvar_tok().find_iter(text) {
        match classes.get(m.start()) {
            Some(&Class::Code) | Some(&Class::Str) => {}
            _ => continue,
        }
        let (line, col) = line_col(text, m.start());
        out.push((line, col, (m.end() - m.start()) as u32));
    }
    out
}

/// Every route-family block definition (`request_route`,
/// `failure_route[x]`, `event_route[mod:event]`, ...).  Unnamed
/// blocks have an empty name: `request_route`/`reply_route` never
/// carry one, `onsend_route` only optionally (cfg.y accepts
/// `onsend_route[NAME]`).
pub fn route_defs(text: &str) -> Vec<Located> {
    route_blocks(text)
        .into_iter()
        .map(|b| Located {
            name: b.name,
            line: b.line,
            col: b.col,
        })
        .collect()
}

/// A route-family block definition with its full brace extent.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Route name; empty for unnamed blocks (`request_route`, ...).
    pub name: String,
    /// Block keyword (`request_route`, `failure_route`, ...).
    pub kind: String,
    /// 0-based line of the keyword.
    pub line: u32,
    /// 0-based start column of the keyword.
    pub col: u32,
    /// 0-based line of the closing brace (last line if unterminated).
    pub end_line: u32,
    /// 0-based column just past the closing brace.
    pub end_col: u32,
    /// 0-based line of the route NAME (keyword line if unnamed).
    pub name_line: u32,
    /// 0-based start column of the route NAME (keyword col if unnamed).
    pub name_col: u32,
}

/// [`route_defs`] with block extents: braces are matched through the
/// comment/string classifier, so a `}` in a string or comment does not
/// close a block; an unterminated block extends to end of text.
pub fn route_blocks(text: &str) -> Vec<Block> {
    let classes = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    for c in re_route_def().captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = whole.start();
        if classes.get(start) != Some(&Class::Code) {
            continue;
        }
        // reject matches that are a tail of a longer identifier
        if start > 0 && is_word(b[start - 1]) {
            continue;
        }
        // the match ends just past the opening brace
        let mut depth = 1usize;
        let mut i = whole.end();
        let mut close = None;
        while i < b.len() {
            if classes[i] == Class::Code {
                match b[i] {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            i += 1;
        }
        let (line, col) = line_col(text, start);
        let (end_line, end_col) = match close {
            Some(p) => {
                let (l, c2) = line_col(text, p);
                (l, c2 + 1)
            }
            None => line_col(text, text.len()),
        };
        let (name_line, name_col) = c
            .get(2)
            .map(|m| line_col(text, m.start()))
            .unwrap_or((line, col));
        out.push(Block {
            name: c.get(2).map(|m| m.as_str().to_string()).unwrap_or_default(),
            kind: c.get(1).unwrap().as_str().to_string(),
            line,
            col,
            end_line,
            end_col,
            name_line,
            name_col,
        });
    }
    out
}

/// Every `route(name)` call site (excluding `*_route(...)` look-alikes).
pub fn route_refs(text: &str) -> Vec<Located> {
    let classes = classify(text);
    let re = re_route_ref();
    let mut out = Vec::new();
    for c in re.captures_iter(text) {
        let whole = c.get(0).unwrap();
        let start = whole.start();
        if classes.get(start) != Some(&Class::Code) {
            continue;
        }
        // exclude failure_route(...), onreply_route(...) etc.
        if start > 0 && is_word(text.as_bytes()[start - 1]) {
            continue;
        }
        let name = c.get(1).unwrap();
        let (line, col) = line_col(text, name.start());
        out.push(Located {
            name: name.as_str().to_string(),
            line,
            col,
        });
    }
    out
}

/// If the cursor (end of `line_prefix`) sits inside the *second*
/// string argument of a `modparam(...)`, return the module name from
/// the first argument.
pub fn modparam_context(line_prefix: &str) -> Option<String> {
    let re = re_modparam_ctx();
    re.captures(line_prefix).map(|c| c[1].to_string())
}

/// The `[A-Za-z0-9_]+` identifier covering byte column `col`, if any.
pub fn word_at(line: &str, col: usize) -> Option<String> {
    let b = line.as_bytes();
    if col >= b.len() || !is_word(b[col]) {
        return None;
    }
    let mut s = col;
    while s > 0 && is_word(b[s - 1]) {
        s -= 1;
    }
    let mut e = col;
    while e < b.len() && is_word(b[e]) {
        e += 1;
    }
    std::str::from_utf8(&b[s..e]).ok().map(|w| w.to_string())
}

/// Convert an LSP UTF-16 code-unit column to a byte offset in `line`,
/// clamping past-end positions to the line length and positions
/// inside a surrogate pair to the character's start.
pub fn utf16_to_byte(line: &str, utf16_col: u32) -> usize {
    let mut units = 0u32;
    for (byte_idx, ch) in line.char_indices() {
        if units >= utf16_col {
            return byte_idx;
        }
        let w = ch.len_utf16() as u32;
        if units + w > utf16_col {
            // inside a surrogate pair: clamp to the char start
            return byte_idx;
        }
        units += w;
    }
    line.len()
}

/// Convert a byte offset in `line` to an LSP UTF-16 code-unit column,
/// clamping past-end offsets to the line's length in units.
pub fn byte_to_utf16(line: &str, byte_col: usize) -> u32 {
    let mut units = 0u32;
    for (byte_idx, ch) in line.char_indices() {
        if byte_idx >= byte_col {
            return units;
        }
        units += ch.len_utf16() as u32;
    }
    units
}
