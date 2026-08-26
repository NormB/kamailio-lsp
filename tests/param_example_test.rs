//! A core parameter's hover carries its worked example.
//!
//! GIVEN the cookbook documents 285 of its 330 core sections with an
//! example of usage,
//! WHEN a reader hovers the parameter,
//! THEN they get it — the example is where the FORM is, and a
//! description of what a setting means does not say what to type.
//!
//! None of them arrived: the walk skips fenced blocks outright, so
//! every one of those examples was dropped.
//!
//! This cookbook is not the OpenSIPS manual and must not be read as
//! if it were. There, a default sits on its own `Default value is
//! ...` line and has to be lifted out separately; here it is written
//! INSIDE the first paragraph — "Default value is 8." in `children` —
//! so it already survives, and synthesising a `Default:` line would
//! either duplicate it or invent one.

mod common;

use kamailio_lsp::catalog::parse_core_cookbook_md;

const PAGE: &str = r#"# Core

## Core parameters

### children

Number of children to fork. Default value is 8.

A second paragraph that is not the summary.

Example of usage:

``` c
children=16
```

### chroot

The chroot directory.

``` c
chroot=/var/lib/kamailio
```

### no_example

Just a description.

### many_examples

A setting the cookbook illustrates several times.

``` c
many_examples=1
```

And another form:

``` c
many_examples=2
```

## Core Functions

### forward(uri)

Forwards the request.

``` c
forward("192.0.2.1");
```
"#;

fn param<'a>(v: &'a [kamailio_lsp::catalog::Item], n: &str) -> &'a kamailio_lsp::catalog::Item {
    v.iter()
        .find(|i| i.name == n)
        .unwrap_or_else(|| panic!("{n} missing"))
}

#[test]
fn the_hover_carries_the_worked_example() {
    let (params, _) = parse_core_cookbook_md(PAGE).expect("parses");
    let ch = param(&params, "children");
    assert!(
        ch.doc.contains("children=16"),
        "the example is what tells a reader the form: {:?}",
        ch.doc
    );
}

#[test]
fn the_description_still_comes_first_and_keeps_its_default() {
    let (params, _) = parse_core_cookbook_md(PAGE).expect("parses");
    let ch = param(&params, "children");
    assert!(
        ch.doc.starts_with("Number of children to fork."),
        "{:?}",
        ch.doc
    );
    assert!(
        ch.doc.contains("Default value is 8."),
        "this cookbook writes the default inside the first paragraph, so it \
         arrives on its own: {:?}",
        ch.doc
    );
    assert!(
        !ch.doc.contains("not the summary"),
        "and the summary is still one paragraph: {:?}",
        ch.doc
    );
}

#[test]
fn no_default_line_is_synthesised() {
    let (params, _) = parse_core_cookbook_md(PAGE).expect("parses");
    let ch = param(&params, "children");
    assert!(
        !ch.doc.contains("Default:"),
        "the OpenSIPS manual needs a `Default:` lifted out of its own line; this \
         one does not, and inventing one would duplicate what the paragraph \
         already says: {:?}",
        ch.doc
    );
}

#[test]
fn an_example_with_no_lead_in_sentence_is_still_taken() {
    let (params, _) = parse_core_cookbook_md(PAGE).expect("parses");
    let ch = param(&params, "chroot");
    assert!(ch.doc.contains("chroot=/var/lib/kamailio"), "{:?}", ch.doc);
}

#[test]
fn a_section_with_no_example_is_unchanged() {
    let (params, _) = parse_core_cookbook_md(PAGE).expect("parses");
    let n = param(&params, "no_example");
    assert_eq!(n.doc.trim(), "Just a description.");
}

#[test]
fn one_section_does_not_borrow_the_next_ones_example() {
    let (params, _) = parse_core_cookbook_md(PAGE).expect("parses");
    let n = param(&params, "no_example");
    assert!(!n.doc.contains("forward"), "{:?}", n.doc);
}

#[test]
fn core_functions_get_their_example_too() {
    let (_, functions) = parse_core_cookbook_md(PAGE).expect("parses");
    let f = param(&functions, "forward");
    assert!(f.doc.contains("forward(\"192.0.2.1\")"), "{:?}", f.doc);
}

/// The real cookbook, not a fixture.
#[test]
fn the_real_cookbook_gives_its_parameters_their_examples() {
    let wiki = common::required_env("KAMAILIO_LSP_TEST_WIKI");
    let core = kamailio_lsp::catalog::harvest_core(std::path::Path::new(&wiki));

    // POSITIVE CONTROL: an empty harvest must not read as "no examples".
    assert!(core.params.len() > 100, "only {} params", core.params.len());
    let with = core.params.iter().filter(|p| p.doc.contains("```")).count();
    assert!(
        with >= 150,
        "only {with} of {} parameters carry their example",
        core.params.len()
    );
    let listen = param(&core.params, "listen");
    assert!(
        listen.doc.contains("listen="),
        "`listen` is the one every configuration starts with: {:?}",
        listen.doc
    );
}

/// The `listen` section alone carries eight fenced blocks. A hover
/// shows ONE example; concatenating every block in a section turns
/// the popup into the manual page it came from.
#[test]
fn only_the_first_example_of_a_section_is_taken() {
    let (params, _) = parse_core_cookbook_md(PAGE).expect("parses");
    let m = param(&params, "many_examples");
    assert!(m.doc.contains("many_examples=1"), "{:?}", m.doc);
    assert!(
        !m.doc.contains("many_examples=2"),
        "the second block is a variation, not part of the first: {:?}",
        m.doc
    );
}

/// Owed, first. The example is shown verbatim.
///
/// The description is sanitised — HTML stripped, non-web links
/// flattened — because it is rendered as prose. An example is CODE:
/// a `<` or a bracketed word in it is what the reader must type, and
/// running the sanitiser over it would quietly edit the answer.
#[test]
fn the_example_is_not_sanitised_like_prose() {
    const MD: &str = concat!(
        "## Core parameters\n\n### tagged\n\nA setting.\n\n",
        "``` c\ntagged = \"<sip:user@example.com>\"\n```\n"
    );
    let (params, _) = parse_core_cookbook_md(MD).expect("parses");
    let it = param(&params, "tagged");
    assert!(
        it.doc.contains("<sip:user@example.com>"),
        "the angle brackets are part of what to type, not markup to strip: {:?}",
        it.doc
    );
}

/// Owed, second. A `#` at column zero inside an example is a
/// CONFIGURATION COMMENT, and markdown reads it as a heading. Ending
/// the section there drops the rest of the example and can start a
/// section named after a line of sample configuration.
#[test]
fn a_comment_at_column_zero_inside_an_example_does_not_end_the_section() {
    const MD: &str = concat!(
        "## Core parameters\n\n### commented\n\nA setting.\n\n",
        "``` c\n# how to set it\ncommented = 1\n```\n"
    );
    let (params, _) = parse_core_cookbook_md(MD).expect("parses");
    let names: Vec<&str> = params.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["commented"], "{names:?}");
    let it = param(&params, "commented");
    assert!(
        it.doc.contains("# how to set it"),
        "the comment belongs to the example: {:?}",
        it.doc
    );
    assert!(
        it.doc.contains("commented = 1"),
        "and so does the line after it: {:?}",
        it.doc
    );
}
