//! An `##` pseudo-variable must hover with its own prose.
//!
//! GIVEN `pseudovariables.md` mixes heading levels — most variables
//! are `###` under a category, but `$avp`, `$var`, `$hdr` and the
//! other containers get an `##` section of their own,
//! WHEN a reader hovers `$avp(id)`,
//! THEN they get what that section says.
//!
//! They got a header and nothing under it. The walker emitted the
//! `##` heading as an item the moment it read the heading line — so
//! the entry existed, appeared in completion, and carried an empty
//! document, because the section's body had not been read yet and was
//! never collected. A blank hover reads as a broken server, and the
//! variables it happened to hit are the ones configurations use most.

mod common;

use kamailio_lsp::catalog::parse_pvars_md;

const PAGE: &str = r#"# Pseudo-Variables

## $avp(id) - AVPs

**$avp(id)** - the value of the AVP identified by 'id'.

A second paragraph that is not the summary.

## $var(name) - Private memory variables

**$var(name)** - private memory variables, persistent per process.

## Message and Transaction

### $ru - Request URI

The request URI of the SIP message.
"#;

#[test]
fn an_h2_variable_carries_the_prose_under_its_heading() {
    let vars = parse_pvars_md(PAGE).expect("the page parses");
    let avp = vars
        .iter()
        .find(|v| v.name == "$avp")
        .expect("$avp is an entry");
    assert!(
        !avp.doc.trim().is_empty(),
        "an entry with no document hovers blank, which reads as the server \
         being broken rather than as the page being thin"
    );
    assert!(
        avp.doc.contains("the value of the AVP identified"),
        "and the document is the section's own prose: {:?}",
        avp.doc
    );
}

#[test]
fn the_prose_stops_at_the_next_heading() {
    let vars = parse_pvars_md(PAGE).expect("the page parses");
    let avp = vars.iter().find(|v| v.name == "$avp").unwrap();
    assert!(
        !avp.doc.contains("not the summary"),
        "the first paragraph is the summary: {:?}",
        avp.doc
    );
    assert!(
        !avp.doc.contains("private memory"),
        "and it must not run into the next variable: {:?}",
        avp.doc
    );
}

#[test]
fn an_h3_variable_still_carries_its_own_prose() {
    let vars = parse_pvars_md(PAGE).expect("the page parses");
    let ru = vars.iter().find(|v| v.name == "$ru").expect("$ru is read");
    assert!(
        ru.doc.contains("The request URI"),
        "the `###` form must keep working: {:?}",
        ru.doc
    );
}

#[test]
fn a_category_heading_that_is_not_a_variable_yields_nothing() {
    let vars = parse_pvars_md(PAGE).expect("the page parses");
    let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
    assert!(
        !names.iter().any(|n| n.starts_with("$Message")),
        "`## Message and Transaction` names no variable: {names:?}"
    );
}

/// The real wiki page, not a fixture.
#[test]
fn the_real_page_documents_the_container_variables() {
    let wiki = common::required_env("KAMAILIO_LSP_TEST_WIKI");
    let core = kamailio_lsp::catalog::harvest_core(std::path::Path::new(&wiki));

    // POSITIVE CONTROL: an empty harvest must not read as a few misses.
    assert!(
        core.pvars.len() > 100,
        "only {} variables harvested",
        core.pvars.len()
    );
    for want in ["$avp", "$var", "$hdr", "$sht"] {
        let it = core
            .pvars
            .iter()
            .find(|v| v.name == want)
            .unwrap_or_else(|| panic!("{want} not harvested"));
        assert!(
            !it.doc.trim().is_empty(),
            "{want} hovers blank: detail {:?}",
            it.detail
        );
    }
}

/// The shipped catalogue carries them documented, with no checkout.
#[test]
fn the_built_in_catalogue_documents_the_container_variables() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    for want in ["$avp", "$var", "$hdr"] {
        let it = core
            .pvars
            .iter()
            .find(|v| v.name == want)
            .unwrap_or_else(|| panic!("{want} missing"));
        assert!(!it.doc.trim().is_empty(), "{want} hovers blank");
    }
}

#[test]
fn hovering_avp_and_var_answers_with_documentation() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let body = "request_route {\n    $var(a) = 1;\n    $avp(b) = $var(a);\n}\n";
    for (word, line, col) in [("$var", 1u32, 4u32), ("$avp", 2, 4)] {
        let got = kamailio_lsp::logic::hover_markdown_at(&[], core, body, word, line, col)
            .unwrap_or_else(|| panic!("{word} must hover"));
        assert!(
            got.len() > word.len() + 40,
            "the hover must carry documentation, not just a header: {got:?}"
        );
    }
}
