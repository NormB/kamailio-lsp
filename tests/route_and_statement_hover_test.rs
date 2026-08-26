//! Route types and control statements hover, and complete with text.
//!
//! GIVEN `request_route`, `branch_route`, `if` and `while` are what a
//! configuration is built out of,
//! WHEN a reader hovers one,
//! THEN they get its documentation — as core functions and
//! pseudo-variables already do.
//!
//! They got nothing. The cookbook documents all of them, under
//! `## Routing Blocks` and `## Script Statements` in `core.md`, and
//! the harvester read the parameter and function sections of that
//! same page and walked past both. Control keywords completed with
//! `detail: "keyword"` and no text at all.
//!
//! Two shapes to be careful of. `### route block` is not an
//! identifier, so the name is its first word and the heading survives
//! as the detail; and `route` is a route type AND a core function, so
//! at a definition the block is what the reader is looking at while
//! at a call site the function is.

mod common;

use kamailio_lsp::catalog::{harvest_core, parse_core_blocks_md};

const PAGE: &str = r#"# Core

## Routing Blocks

Intro prose that belongs to no entry.

### request_route

Request routing block - is executed for each SIP request.

A second paragraph that is not the summary.

### route block

Sub-route blocks, invoked from another route block like a function.

### branch_route

Executed for each branch of a request.

### onsend_route

## Core Keywords

### method

The SIP method of the request.

## Script Statements

### if

IF-ELSE statement

### while

WHILE statement

## Core Functions

### forward(uri)

Forwards the request.
"#;

#[test]
fn the_routing_blocks_section_yields_one_entry_per_heading() {
    let (routes, _) = parse_core_blocks_md(PAGE).expect("the page parses");
    let names: Vec<&str> = routes.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["request_route", "route", "branch_route"]);
}

#[test]
fn a_heading_that_is_not_an_identifier_keeps_its_first_word() {
    let (routes, _) = parse_core_blocks_md(PAGE).expect("the page parses");
    let route = routes.iter().find(|r| r.name == "route").expect("route");
    assert!(
        route.detail.contains("route block"),
        "the heading as written is what tells a reader which one it is: {:?}",
        route.detail
    );
    assert!(
        route.doc.contains("Sub-route blocks"),
        "and it carries its own text: {:?}",
        route.doc
    );
}

#[test]
fn the_summary_is_the_first_paragraph_only() {
    let (routes, _) = parse_core_blocks_md(PAGE).expect("the page parses");
    let rr = routes.iter().find(|r| r.name == "request_route").unwrap();
    assert!(rr.doc.starts_with("Request routing block"), "{:?}", rr.doc);
    assert!(
        !rr.doc.contains("not the summary"),
        "it stops at the paragraph break: {:?}",
        rr.doc
    );
}

#[test]
fn the_script_statements_section_yields_the_control_keywords() {
    let (_, statements) = parse_core_blocks_md(PAGE).expect("the page parses");
    let names: Vec<&str> = statements.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"if"), "{names:?}");
    assert!(names.contains(&"while"), "{names:?}");
    let if_ = statements.iter().find(|s| s.name == "if").unwrap();
    assert!(!if_.doc.trim().is_empty(), "with its text");
}

/// `else`, `case` and `default` have no section of their own: they
/// are explained inside the statement they belong to, so they carry
/// that section's text rather than hovering nothing.
#[test]
fn a_keyword_explained_under_another_carries_that_text() {
    let (_, statements) = parse_core_blocks_md(PAGE).expect("the page parses");
    let else_ = statements
        .iter()
        .find(|s| s.name == "else")
        .expect("else is aliased to if");
    let if_ = statements.iter().find(|s| s.name == "if").unwrap();
    assert_eq!(else_.doc, if_.doc);
    assert!(
        else_.detail.contains("if"),
        "and says where it came from: {:?}",
        else_.detail
    );
}

/// `core.md` is one page carrying keywords, values, parameters,
/// functions, blocks and statements. Only two of those sections are
/// read here, and `### method` under "Core Keywords" is a heading of
/// exactly the shape a routing block has — so a filter that leans on
/// the heading's shape rather than the section it is in sweeps it up.
#[test]
fn other_sections_of_the_same_page_are_not_swept_in() {
    let (routes, statements) = parse_core_blocks_md(PAGE).expect("the page parses");
    let names: Vec<&str> = routes
        .iter()
        .chain(statements.iter())
        .map(|i| i.name.as_str())
        .collect();
    assert!(
        !names.contains(&"method"),
        "a core keyword is not a routing block: {names:?}"
    );
    assert!(
        !names.contains(&"forward"),
        "nor is a core function: {names:?}"
    );
}

/// A heading with nothing under it is not documentation: it hovers a
/// header over an empty body, which reads as the server being broken.
#[test]
fn a_block_with_no_prose_is_not_offered() {
    let (routes, _) = parse_core_blocks_md(PAGE).expect("the page parses");
    let names: Vec<&str> = routes.iter().map(|r| r.name.as_str()).collect();
    assert!(
        !names.contains(&"onsend_route"),
        "the fixture leaves this one empty on purpose: {names:?}"
    );
    // POSITIVE CONTROL: the ones with text are still there, so this is
    // not passing on a parser that read nothing
    assert!(names.contains(&"branch_route"), "{names:?}");
}

#[test]
fn adversarial_pages_do_not_panic() {
    for md in ["", "\0", "## Routing Blocks", "### if\n\ntext\n"] {
        let _ = parse_core_blocks_md(md);
    }
}

#[test]
fn the_real_cookbook_documents_every_routing_block() {
    let wiki = common::required_env("KAMAILIO_LSP_TEST_WIKI");
    let core = harvest_core(std::path::Path::new(&wiki));

    // POSITIVE CONTROL: an empty harvest would fail every check below
    // as a pile of misses rather than as the one thing that is wrong.
    assert!(
        core.routes.len() >= 8,
        "only {} routing blocks harvested: {:?}",
        core.routes.len(),
        core.routes.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
    let names: Vec<&str> = core.routes.iter().map(|r| r.name.as_str()).collect();
    for kind in [
        "request_route",
        "route",
        "branch_route",
        "failure_route",
        "reply_route",
        "onreply_route",
        "onsend_route",
        "event_route",
    ] {
        assert!(names.contains(&kind), "{kind} is not documented: {names:?}");
        let it = core.routes.iter().find(|r| r.name == kind).unwrap();
        assert!(!it.doc.trim().is_empty(), "{kind} has an empty summary");
    }
    for kw in ["if", "switch", "while", "else"] {
        let it = core
            .statements
            .iter()
            .find(|s| s.name == kw)
            .unwrap_or_else(|| panic!("{kw} is not documented"));
        assert!(!it.doc.trim().is_empty(), "{kw} has an empty summary");
    }
}

#[test]
fn the_built_in_catalogue_carries_them() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let names: Vec<&str> = core.routes.iter().map(|r| r.name.as_str()).collect();
    assert!(names.len() >= 8, "{names:?}");
    for kind in ["request_route", "branch_route", "event_route"] {
        assert!(names.contains(&kind), "{kind} missing: {names:?}");
    }
    assert!(core.statements.iter().any(|s| s.name == "if"), "if missing");
}

#[test]
fn hovering_a_routing_block_answers_with_its_documentation() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    for kind in [
        "request_route",
        "branch_route",
        "event_route",
        "failure_route",
    ] {
        let body = format!("{kind} {{\n    xlog(\"x\");\n}}\n");
        let got = kamailio_lsp::logic::hover_markdown_at(&[], core, &body, kind, 0, 0)
            .unwrap_or_else(|| panic!("{kind} must hover"));
        assert!(got.contains(kind), "must name it: {got:?}");
        assert!(
            got.to_lowercase().contains("routing block"),
            "and say what it is: {got:?}"
        );
    }
}

#[test]
fn hovering_a_control_statement_answers_with_its_documentation() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let body = "request_route {\n    if (is_method(\"INVITE\")) { exit; }\n}\n";
    for kw in ["if", "while"] {
        let got = kamailio_lsp::logic::hover_markdown_at(&[], core, body, kw, 1, 4)
            .unwrap_or_else(|| panic!("{kw} must hover"));
        assert!(
            got.len() > kw.len() + 20,
            "documentation, not just a header: {got:?}"
        );
    }
}

#[test]
fn control_keywords_complete_with_their_text() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let offered =
        kamailio_lsp::logic::completions_with_core(&[], core, "request_route {\n}\n", "    i");
    let if_ = offered
        .iter()
        .find(|c| c.label == "if")
        .expect("if is offered");
    assert_ne!(
        if_.detail, "keyword",
        "a documented keyword must carry its text into the popup, not the \
         word `keyword`"
    );
    assert!(!if_.doc.trim().is_empty(), "and its documentation");
}
