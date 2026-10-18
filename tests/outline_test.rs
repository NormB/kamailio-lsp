//! The outline, folding and selection ranges: what a reader navigates
//! a config by.
//!
//! A route body is where a large config's length is, so the blocks
//! inside a route nest under it in the outline and fold on their own;
//! `loadmodule`, `modparam`, top-level parameter assignments and
//! `#!define`d names are listed beside the route blocks.  Route
//! definitions cannot nest, and a `modparam` need not sit next to its
//! `loadmodule`, so neither is nested under another: an outline parent
//! must contain its children.  Each branch of an `#!ifdef` folds.

use kamailio_lsp::outline::{FoldKind, Sym, SymKind, folds, outline, selection_chain};

const CFG: &str = r#"#!define WITH_X 1
loadmodule "tm.so"
modparam("tm", "fr_timer", 5)
debug = 3
request_route {
    if (is_method("INVITE")) {
        switch ($rs) {
            case 1:
                exit;
        }
    } else {
        exit;
    }
}
# a comment
// that runs on
route[RELAY] {
    /* one
       two */
    t_relay();
}
#!ifdef WITH_X
route[X] {
    exit;
}
#!else
route[Y] {
    exit;
}
#!endif
"#;

fn flat(syms: &[Sym], depth: usize, out: &mut Vec<String>) {
    for s in syms {
        out.push(format!(
            "{}{} {:?} {}:{}-{}:{} sel {}:{}-{}:{}",
            "  ".repeat(depth),
            s.name,
            s.kind,
            s.span.start_line,
            s.span.start_col,
            s.span.end_line,
            s.span.end_col,
            s.sel.start_line,
            s.sel.start_col,
            s.sel.end_line,
            s.sel.end_col
        ));
        flat(&s.children, depth + 1, out);
    }
}

#[test]
fn the_outline_nests_the_blocks_inside_a_route() {
    let mut got = Vec::new();
    flat(&outline(CFG), 0, &mut got);
    assert_eq!(
        got,
        vec![
            "WITH_X Constant 0:9-0:15 sel 0:9-0:15",
            "tm Module 1:0-1:18 sel 1:12-1:17",
            "fr_timer Property 2:0-2:29 sel 2:16-2:24",
            "debug Variable 3:0-3:9 sel 3:0-3:5",
            "request_route Route 4:0-13:1 sel 4:0-4:13",
            "  if (is_method(\"INVITE\")) Block 5:4-10:5 sel 5:4-5:6",
            "    switch ($rs) Block 6:8-9:9 sel 6:8-6:14",
            "  else Block 10:6-12:5 sel 10:6-10:10",
            "route[RELAY] Route 16:0-20:1 sel 16:6-16:11",
            "route[X] Route 22:0-24:1 sel 22:6-22:7",
            "route[Y] Route 26:0-28:1 sel 26:6-26:7",
        ]
    );
}

#[test]
fn a_define_and_a_modparam_carry_their_detail() {
    let syms = outline(CFG);
    let d = syms.iter().find(|s| s.name == "WITH_X").unwrap();
    assert_eq!(d.detail.as_deref(), Some("#!define 1"));
    let p = syms.iter().find(|s| s.name == "fr_timer").unwrap();
    assert_eq!(p.detail.as_deref(), Some("tm"));
}

#[test]
fn the_bracketed_loadmodule_spans_its_parentheses() {
    let t = "loadmodule(\"tm.so\", \"opt\")\nloadmodulex \"sl.so\"\n";
    let got: Vec<(String, u32)> = outline(t)
        .into_iter()
        .map(|s| (s.name, s.span.end_col))
        .collect();
    assert_eq!(got, vec![("tm".to_string(), 26), ("sl".to_string(), 19)]);
}

#[test]
fn only_top_level_assignments_are_parameters() {
    // inside a route, a line that opens with `word =` is not a global
    // parameter; a dotted name is a `group.var` config variable
    let t = "tls.cfg_file = \"x\"\nrequest_route {\n    $var(x) = 1;\n    if ($var(x) == 1 &&\n    stray = 2) { exit; }\n}\n";
    let names: Vec<String> = outline(t)
        .into_iter()
        .filter(|s| s.kind == SymKind::Variable)
        .map(|s| s.name)
        .collect();
    assert_eq!(names, vec!["tls.cfg_file"]);
}

#[test]
fn folds_cover_blocks_comments_and_conditional_branches() {
    let got: Vec<(u32, u32, FoldKind)> = folds(CFG)
        .into_iter()
        .map(|f| (f.start_line, f.end_line, f.kind))
        .collect();
    assert_eq!(
        got,
        vec![
            (4, 13, FoldKind::Region),
            // `} else {` closes this block on a line that opens the
            // next, so the fold stops above it and `else` stays visible
            (5, 9, FoldKind::Region),
            (6, 9, FoldKind::Region),
            (10, 12, FoldKind::Region),
            (14, 15, FoldKind::Comment),
            (16, 20, FoldKind::Region),
            (17, 18, FoldKind::Comment),
            // each branch of the conditional folds on its own
            (21, 24, FoldKind::Region),
            (22, 24, FoldKind::Region),
            (25, 28, FoldKind::Region),
            (26, 28, FoldKind::Region),
        ]
    );
}

#[test]
fn a_brace_in_a_string_or_comment_opens_no_block() {
    let t =
        "request_route {\n    if ($rU) {\n        xlog(\"}\");\n        exit;\n    }\n    # {\n}\n";
    let got: Vec<(u32, u32)> = folds(t)
        .into_iter()
        .map(|f| (f.start_line, f.end_line))
        .collect();
    assert_eq!(got, vec![(0, 6), (1, 4)]);
}

#[test]
fn a_comment_or_directive_above_a_block_is_not_part_of_its_name() {
    let t = "request_route {\n    exit;\n    # only INVITEs\n#!ifdef X\n    if (is_method(\"INVITE\")) {\n        exit;\n    }\n#!endif\n}\n";
    let route = outline(t).pop().unwrap();
    let b = &route.children[0];
    assert_eq!(b.name, "if (is_method(\"INVITE\"))");
    assert_eq!((b.span.start_line, b.span.start_col), (4, 4));
}

#[test]
fn selection_grows_from_the_word_to_the_route() {
    // the cursor on `rs` in `switch ($rs)`
    let line6 = CFG.lines().nth(6).unwrap();
    let off = CFG.lines().take(6).map(|l| l.len() + 1).sum::<usize>() + line6.find("rs").unwrap();
    let got: Vec<String> = selection_chain(CFG, off)
        .into_iter()
        .map(|s| {
            format!(
                "{}:{}-{}:{}",
                s.start_line, s.start_col, s.end_line, s.end_col
            )
        })
        .collect();
    assert_eq!(
        got,
        vec!["6:17-6:19", "6:8-9:9", "5:4-10:5", "4:0-13:1", "0:0-30:0"]
    );
}

#[test]
fn a_socket_definition_is_one_entry_and_folds() {
    // `socket = { ... }` (Kamailio 6) has braces at the top level that
    // are not a route body
    let t = "socket = {\n    bind = udp:192.0.2.1:5060;\n    name = \"s1\";\n}\nrequest_route {\n    exit;\n}\n";
    let mut got = Vec::new();
    flat(&outline(t), 0, &mut got);
    assert_eq!(
        got,
        vec![
            "socket Variable 0:0-3:1 sel 0:0-0:6",
            "request_route Route 4:0-6:1 sel 4:0-4:13",
        ]
    );
    let f: Vec<(u32, u32)> = folds(t)
        .into_iter()
        .map(|f| (f.start_line, f.end_line))
        .collect();
    assert_eq!(f, vec![(0, 3), (4, 6)]);
}

/// A `//` inside `$(...)` is pseudo-variable text (cfg.lex state
/// PVAR_P), so the route it sits in ends at its own brace and the
/// route after it is still in the outline.
#[test]
fn a_pseudo_variable_with_slashes_does_not_swallow_the_next_route() {
    let t = "request_route {\n    $ru = $(ru{re.subst,/;t=[a-z]*//g});\n}\nroute[NEXT] {\n    exit;\n}\n";
    let mut got = Vec::new();
    flat(&outline(t), 0, &mut got);
    assert_eq!(
        got,
        vec![
            "request_route Route 0:0-2:1 sel 0:0-0:13",
            "route[NEXT] Route 3:0-5:1 sel 3:6-3:10",
        ]
    );
}
