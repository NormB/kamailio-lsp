//! `listen=` modifiers and `socket = { }` attributes.
//!
//! GIVEN Kamailio has TWO ways to describe a listening socket — the
//! space-separated `listen=udp:… advertise … name "…" virtual` and
//! the structured `socket = { bind = …; advertise = …; }` block,
//! WHEN a reader hovers `advertise`, `name`, `virtual`, `bind`,
//! `agname`, `workers` or `vrf`,
//! THEN they get what that attribute does.
//!
//! They got nothing.
//!
//! Neither syntax is the sibling server's. OpenSIPS takes bare
//! space-separated flags after the address (`use_workers 4`,
//! `reuse_port`) and has no brace form at all; the names, the shapes
//! and the separators here are Kamailio's, and are read out of
//! Kamailio's own grammar. The lexer dialect differs too: this one
//! writes its alternatives UNQUOTED — `advertise|ADVERTISE`,
//! `name|NAME` — where the other quotes them.
//!
//! The grammar decides membership and the cookbook supplies the
//! text. Those two disagree here, and the grammar wins: `workers` is
//! accepted inside a `socket` block and the cookbook's attribute list
//! does not mention it.

mod common;

use kamailio_lsp::catalog::{parse_socket_attrs_c, parse_socket_attrs_md};

const LEX: &str = r#"
ADVERTISE	advertise|ADVERTISE
VIRTUAL		virtual
STRNAME		name|NAME
AGNAME		agname|AGNAME
VRF		vrf|VRF
BIND bind
WORKERS workers
%%
"#;

const Y: &str = r#"
socket_lattr:
	BIND EQUAL listen_id	{ x; }
	| STRNAME EQUAL STRING { x; }
	| ADVERTISE EQUAL listen_id COLON NUMBER { x; }
	| AGNAME EQUAL STRING { x; }
	| WORKERS EQUAL NUMBER { x; }
	| VIRTUAL EQUAL NUMBER { x; }
	| VRF EQUAL STRING { x; }
	| SEMICOLON {}
	;
socket_lattrs:
	socket_lattrs socket_lattr {}
	;
"#;

const PAGE: &str = r#"# Core

## Core parameters

### socket

Specify an address to listen (bind) to.

The attributes are:

- `bind` - the address to listen on in format `[proto:]address[:port]`
- `advertise` - the address to advertise in SIP headers
- `name` - name of the socket to be referenced in configuration file
- `agname` - async group name
- `virtual` - set to `yes/no` to indicate if the IP has to be considered virtual
- `vrf` - name of the VRF device associated with this socket

``` c
socket = {
    bind = udp:192.0.2.1:5060;
}
```

### listen

Set the network addresses to listen to.

``` c
    listen=udp:192.0.2.1:5060
```
"#;

#[test]
fn membership_comes_from_the_grammar() {
    let got = parse_socket_attrs_c(Y, LEX);
    assert_eq!(
        got,
        vec![
            "bind",
            "name",
            "advertise",
            "agname",
            "workers",
            "virtual",
            "vrf"
        ],
        "in the order the production lists them"
    );
}

/// This lexer writes its alternatives without quotes.
#[test]
fn an_unquoted_alternation_is_read() {
    let got = parse_socket_attrs_c(Y, LEX);
    assert!(
        got.contains(&"advertise".to_string()),
        "`ADVERTISE\tadvertise|ADVERTISE` is `advertise`; a reader written for \
         quoted alternatives finds nothing here: {got:?}"
    );
    assert!(
        !got.iter()
            .any(|g| g.contains('|') || g.contains("ADVERTISE")),
        "and it takes the lower-case half, not the raw pattern: {got:?}"
    );
}

#[test]
fn a_production_that_is_not_the_socket_one_is_not_read() {
    assert!(
        parse_socket_attrs_c("other: BIND EQUAL x { y; } ;\n", LEX).is_empty(),
        "only the attribute production of a `socket` block counts"
    );
}

#[test]
fn the_cookbook_supplies_the_text() {
    let got = parse_socket_attrs_md(PAGE);
    let bind = got.iter().find(|i| i.name == "bind").expect("bind");
    assert!(
        bind.doc.contains("the address to listen on"),
        "{:?}",
        bind.doc
    );
    let vrf = got.iter().find(|i| i.name == "vrf").expect("vrf");
    assert!(vrf.doc.contains("VRF device"), "{:?}", vrf.doc);
}

/// The cookbook uses ` - ` between the name and its description, not
/// the `: ` the other server's manual uses.
#[test]
fn the_dash_separated_list_form_is_read() {
    let got = parse_socket_attrs_md(PAGE);
    let name = got.iter().find(|i| i.name == "name").expect("name");
    assert!(
        !name.doc.starts_with('-') && !name.doc.starts_with('`'),
        "the separator must not survive into the text: {:?}",
        name.doc
    );
}

#[test]
fn bullets_outside_the_socket_section_are_not_attributes() {
    let got = parse_socket_attrs_md(
        "## Core parameters\n\n### other\n\nThings:\n- `nope` - no.\n\n### socket\n\nThe attributes are:\n- `bind` - yes.\n",
    );
    let names: Vec<&str> = got.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["bind"], "{names:?}");
}

/// The grammar and the cookbook disagree, and the grammar wins —
/// tested against a HARVEST, because the vendored catalogue below
/// guards the shipped artefact and would not notice this rule
/// changing until the artefact were regenerated.
#[test]
fn a_harvest_notes_the_attribute_the_cookbook_omits() {
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let wiki = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_WIKI"));
    let (attrs, _) = kamailio_lsp::catalog::harvest_socket_syntax(&tree, &wiki);

    // POSITIVE CONTROL: the harvest read something.
    assert!(
        attrs.len() >= 6,
        "{:?}",
        attrs.iter().map(|a| &a.name).collect::<Vec<_>>()
    );
    let w = attrs
        .iter()
        .find(|a| a.name == "workers")
        .expect("`workers` is accepted inside a socket block");
    assert!(
        w.doc.to_lowercase().contains("does not describe"),
        "an entry with no text hovers blank; it must say the cookbook is silent \
         about it rather than pretend it does not exist: {:?}",
        w.doc
    );
    // and a documented one keeps the cookbook's words
    let bind = attrs.iter().find(|a| a.name == "bind").unwrap();
    assert!(
        bind.doc.contains("the address to listen on"),
        "{:?}",
        bind.doc
    );
}

/// The grammar and the cookbook disagree, and the grammar wins.
#[test]
fn an_attribute_the_cookbook_omits_is_still_offered() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let w = core
        .socket_attrs
        .iter()
        .find(|a| a.name == "workers")
        .expect("`workers` is accepted inside a socket block");
    assert!(
        !w.doc.trim().is_empty(),
        "an entry with no text hovers blank; it must say the cookbook does not \
         describe it"
    );
}

#[test]
fn the_real_grammar_and_cookbook_agree_about_the_rest() {
    let tree = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let wiki = std::path::PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_WIKI"));
    let y = std::fs::read_to_string(tree.join("src/core/cfg.y")).expect("cfg.y");
    let lex = std::fs::read_to_string(tree.join("src/core/cfg.lex")).expect("cfg.lex");
    let cookbook = kamailio_lsp::catalog::cookbook_core_md(&wiki).expect("core.md");

    let accepted = parse_socket_attrs_c(&y, &lex);
    let documented: Vec<String> = parse_socket_attrs_md(&cookbook)
        .into_iter()
        .map(|i| i.name)
        .collect();
    // POSITIVE CONTROL: neither scan may be empty.
    assert!(accepted.len() >= 6, "grammar gave {accepted:?}");
    assert!(documented.len() >= 5, "cookbook gave {documented:?}");

    let undocumented: Vec<&String> = accepted
        .iter()
        .filter(|a| !documented.contains(a))
        .collect();
    assert_eq!(
        undocumented,
        vec!["workers"],
        "the known disagreement is `workers` alone; anything else means one of \
         the two moved and the reconciliation needs looking at"
    );
}

#[test]
fn the_built_in_catalogue_carries_both_sets() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let attrs: Vec<&str> = core.socket_attrs.iter().map(|a| a.name.as_str()).collect();
    for a in [
        "bind",
        "advertise",
        "name",
        "agname",
        "workers",
        "virtual",
        "vrf",
    ] {
        assert!(attrs.contains(&a), "{a} missing: {attrs:?}");
    }
    let mut mods: Vec<&str> = core
        .listen_modifiers
        .iter()
        .map(|a| a.name.as_str())
        .collect();
    mods.sort_unstable();
    assert_eq!(
        mods,
        vec!["advertise", "name", "virtual"],
        "the `listen=` line takes these three and no more — compared as a set, \
         because the grammar's line order is not information"
    );
}

#[test]
fn hovering_an_attribute_inside_a_socket_block_answers() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let text =
        "socket = {\n    bind = udp:192.0.2.1:5060;\n    advertise = 198.51.100.1:5060;\n}\n";
    for (w, line, col) in [("bind", 1u32, 4u32), ("advertise", 2, 4)] {
        let got = kamailio_lsp::logic::hover_markdown_at(&[], core, text, w, line, col)
            .unwrap_or_else(|| panic!("{w} must hover inside a socket block"));
        assert!(got.contains(w), "must name it: {got:?}");
        assert!(
            got.to_lowercase().contains("socket"),
            "and say where it belongs: {got:?}"
        );
    }
}

#[test]
fn hovering_a_modifier_on_a_listen_line_answers() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let text = "listen=udp:192.0.2.1:5060 advertise 198.51.100.1:5060 name \"s1\"\n";
    for (w, col) in [("advertise", 26u32), ("name", 54)] {
        let got = kamailio_lsp::logic::hover_markdown_at(&[], core, text, w, 0, col)
            .unwrap_or_else(|| panic!("{w} must hover on a listen line"));
        assert!(got.contains(w), "{got:?}");
    }
}

/// `name` and `virtual` are ordinary words.
#[test]
fn those_words_elsewhere_do_not_hover_as_socket_syntax() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let text = "request_route {\n    $var(name) = 1;\n}\n";
    let got = kamailio_lsp::logic::hover_markdown_at(&[], core, text, "name", 1, 9);
    assert!(
        got.as_deref()
            .is_none_or(|h| !h.to_lowercase().contains("socket attribute")),
        "answering for every `name` in a configuration is worse than answering \
         for none: {got:?}"
    );
}

#[test]
fn a_listen_line_offers_its_three_modifiers() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let offered = kamailio_lsp::logic::completions_with_core(
        &[],
        core,
        "listen=udp:192.0.2.1:5060 \n",
        "listen=udp:192.0.2.1:5060 ",
    );
    let labels: Vec<&str> = offered.iter().map(|c| c.label.as_str()).collect();
    for m in ["advertise", "name", "virtual"] {
        assert!(labels.contains(&m), "{m} not offered: {labels:?}");
    }
    assert!(
        !labels.contains(&"bind"),
        "`bind` belongs to the brace form, not to a `listen=` line: {labels:?}"
    );
}

#[test]
fn an_ordinary_line_offers_neither_set() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let offered =
        kamailio_lsp::logic::completions_with_core(&[], core, "request_route {\n}\n", "    ");
    let labels: Vec<&str> = offered.iter().map(|c| c.label.as_str()).collect();
    assert!(!labels.contains(&"vrf"), "{} labels", labels.len());
    assert!(!labels.contains(&"advertise"), "{} labels", labels.len());
}

/// A `socket = { ... }` written on ONE line closes on that line.
///
/// The opener set the depth and then moved on without counting the
/// braces on its own line, so a single-line block never closed: every
/// line after it in the file read as inside a socket block, and
/// `name`, `virtual`, `workers` and `advertise` hovered as socket
/// syntax in the middle of a route body. The cookbook writes the form
/// across several lines, which is exactly why nothing noticed.
#[test]
fn a_socket_block_written_on_one_line_closes_on_that_line() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let text = "socket = { bind = udp:192.0.2.1:5060; }\nrequest_route {\n    $var(name) = 1;\n}\n";
    let got = kamailio_lsp::logic::hover_markdown_at(&[], core, text, "name", 2, 9);
    assert!(
        got.as_deref()
            .is_none_or(|h| !h.to_lowercase().contains("socket attribute")),
        "the block closed on line 0; `name` here is a variable: {got:?}"
    );
    // POSITIVE CONTROL: inside the one-line block it still answers
    let inside = kamailio_lsp::logic::hover_markdown_at(&[], core, text, "bind", 0, 11);
    assert!(
        inside.is_some_and(|h| h.to_lowercase().contains("socket attribute")),
        "and an attribute ON that line must still hover"
    );
}

/// The same for a block that opens and closes across lines: what
/// follows it is ordinary configuration again.
#[test]
fn a_closed_socket_block_does_not_leak_into_what_follows() {
    let core = &kamailio_lsp::catalog::builtin_core().core;
    let text = "socket = {\n    bind = udp:192.0.2.1:5060;\n}\n\nrequest_route {\n    $var(name) = 1;\n}\n";
    let got = kamailio_lsp::logic::hover_markdown_at(&[], core, text, "name", 5, 9);
    assert!(
        got.as_deref()
            .is_none_or(|h| !h.to_lowercase().contains("socket attribute")),
        "{got:?}"
    );
}
