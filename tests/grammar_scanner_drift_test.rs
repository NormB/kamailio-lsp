//! Grammar ↔ scanner drift gate.
//!
//! The tree-sitter grammar (`tree-sitter-kamailio/`) and the server's
//! regex scanner (`analyze`) model the same language.  This gate
//! parses every corpus case (input + EXPECTED S-expression) and
//! proves both sides agree on the countable constructs: route
//! definitions (total and named), `loadmodule` statements, and
//! `modparam` calls.  Extending the grammar without the scanner — or
//! vice versa — breaks a count and fails here.  Zero new
//! dependencies: the corpus format and the S-expressions are parsed
//! by hand.

use kamailio_lsp::analyze;

/// One corpus case: name, input text, expected S-expression text.
struct Case {
    name: String,
    input: String,
    expected: String,
}

/// Parse the tree-sitter corpus format: `===` fence, name, `===`
/// fence, input, `---` fence, expected S-expression, repeat.
fn parse_corpus(text: &str) -> Vec<Case> {
    let is_eq = |l: &str| l.len() >= 3 && l.chars().all(|c| c == '=');
    let is_dash = |l: &str| l.len() >= 3 && l.chars().all(|c| c == '-');
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !is_eq(lines[i]) {
            i += 1;
            continue;
        }
        let name = lines.get(i + 1).unwrap_or(&"").to_string();
        assert!(
            lines.get(i + 2).map(|l| is_eq(l)).unwrap_or(false),
            "corpus header fence missing after '{name}'"
        );
        i += 3;
        let mut input = String::new();
        while i < lines.len() && !is_dash(lines[i]) {
            input.push_str(lines[i]);
            input.push('\n');
            i += 1;
        }
        assert!(i < lines.len(), "corpus separator missing in '{name}'");
        i += 1; // the --- fence
        let mut expected = String::new();
        while i < lines.len() && !is_eq(lines[i]) {
            expected.push_str(lines[i]);
            expected.push('\n');
            i += 1;
        }
        out.push(Case {
            name,
            input,
            expected,
        });
    }
    out
}

/// A parsed S-expression node: name plus children (leaf fields like
/// `name:` attach to the following node as its field label).
struct Node {
    kind: String,
    field: Option<String>,
    children: Vec<Node>,
}

/// Hand-rolled S-expression parser for the corpus's expected trees.
fn parse_sexp(text: &str) -> Vec<Node> {
    #[derive(Debug, PartialEq)]
    enum Tok {
        Open,
        Close,
        Field(String),
        Sym(String),
    }
    let mut toks = Vec::new();
    let mut cur = String::new();
    let flush = |cur: &mut String, toks: &mut Vec<Tok>| {
        if cur.is_empty() {
            return;
        }
        let t = std::mem::take(cur);
        if let Some(f) = t.strip_suffix(':') {
            toks.push(Tok::Field(f.to_string()));
        } else {
            toks.push(Tok::Sym(t));
        }
    };
    for c in text.chars() {
        match c {
            '(' => {
                flush(&mut cur, &mut toks);
                toks.push(Tok::Open);
            }
            ')' => {
                flush(&mut cur, &mut toks);
                toks.push(Tok::Close);
            }
            c if c.is_whitespace() => flush(&mut cur, &mut toks),
            c => cur.push(c),
        }
    }
    flush(&mut cur, &mut toks);

    fn parse_nodes(toks: &[Tok], i: &mut usize) -> Vec<Node> {
        let mut out = Vec::new();
        let mut pending_field: Option<String> = None;
        while *i < toks.len() {
            match &toks[*i] {
                Tok::Open => {
                    *i += 1;
                    let kind = match toks.get(*i) {
                        Some(Tok::Sym(s)) => {
                            *i += 1;
                            s.clone()
                        }
                        _ => String::new(),
                    };
                    let children = parse_nodes(toks, i);
                    out.push(Node {
                        kind,
                        field: pending_field.take(),
                        children,
                    });
                }
                Tok::Close => {
                    *i += 1;
                    return out;
                }
                Tok::Field(f) => {
                    pending_field = Some(f.clone());
                    *i += 1;
                }
                Tok::Sym(_) => {
                    *i += 1;
                }
            }
        }
        out
    }
    let mut i = 0;
    parse_nodes(&toks, &mut i)
}

/// Count nodes of `kind` anywhere in the trees; when `with_field` is
/// given, count only nodes carrying a child with that field label.
fn count(nodes: &[Node], kind: &str, with_field: Option<&str>) -> usize {
    let mut n = 0;
    for node in nodes {
        if node.kind == kind {
            match with_field {
                None => n += 1,
                Some(f) => {
                    if node.children.iter().any(|c| c.field.as_deref() == Some(f)) {
                        n += 1;
                    }
                }
            }
        }
        n += count(&node.children, kind, with_field);
    }
    n
}

fn corpus_cases() -> Vec<Case> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tree-sitter-kamailio")
        .join("test")
        .join("corpus");
    let mut cases = Vec::new();
    for e in std::fs::read_dir(&dir).expect("corpus dir").flatten() {
        let text = std::fs::read_to_string(e.path()).expect("corpus file");
        cases.extend(parse_corpus(&text));
    }
    assert!(!cases.is_empty(), "no corpus cases found under {dir:?}");
    cases
}

#[test]
fn grammar_and_scanner_agree_on_route_definitions() {
    for case in corpus_cases() {
        let tree = parse_sexp(&case.expected);
        let grammar_defs = count(&tree, "route_definition", None);
        let grammar_named = count(&tree, "route_definition", Some("name"));
        let blocks = analyze::route_blocks(&case.input);
        let scanner_named = blocks.iter().filter(|b| !b.name.is_empty()).count();
        assert_eq!(
            grammar_defs,
            blocks.len(),
            "route-definition count drift in corpus case '{}'\ninput:\n{}",
            case.name,
            case.input
        );
        assert_eq!(
            grammar_named, scanner_named,
            "named-route count drift in corpus case '{}'\ninput:\n{}",
            case.name, case.input
        );
    }
}

#[test]
fn grammar_and_scanner_agree_on_loadmodule_and_modparam() {
    for case in corpus_cases() {
        let tree = parse_sexp(&case.expected);
        let grammar_loads = count(&tree, "loadmodule", None);
        let grammar_modparams = count(&tree, "modparam", None);
        assert_eq!(
            grammar_loads,
            analyze::loaded_modules(&case.input).len(),
            "loadmodule count drift in corpus case '{}'\ninput:\n{}",
            case.name,
            case.input
        );
        assert_eq!(
            grammar_modparams,
            analyze::modparam_calls(&case.input).len(),
            "modparam count drift in corpus case '{}'\ninput:\n{}",
            case.name,
            case.input
        );
    }
}

#[test]
fn the_gate_actually_counts_something() {
    // guard against a silently degenerate gate: the corpus must
    // exercise every counted construct at least once
    let mut defs = 0;
    let mut named = 0;
    let mut loads = 0;
    let mut modparams = 0;
    for case in corpus_cases() {
        let tree = parse_sexp(&case.expected);
        defs += count(&tree, "route_definition", None);
        named += count(&tree, "route_definition", Some("name"));
        loads += count(&tree, "loadmodule", None);
        modparams += count(&tree, "modparam", None);
    }
    assert!(defs >= 5, "corpus too thin: {defs} route definitions");
    assert!(named >= 3, "corpus too thin: {named} named routes");
    assert!(loads >= 3, "corpus too thin: {loads} loadmodules");
    assert!(modparams >= 2, "corpus too thin: {modparams} modparams");
}
