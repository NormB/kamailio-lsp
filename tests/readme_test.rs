//! The module documentation source: Kamailio ships a generated
//! plain-text `README` in every `src/modules/<name>/` directory, with
//! numbered `N. Parameters` / `N. Functions` chapters and
//! `N.M. name (type)` / `N.M.  func(sig)` item headings at column 0
//! (the table of contents repeats them indented). This fixture models
//! the real tm README structure.

use kamailio_lsp::catalog::parse_readme_txt;

const FIXTURE: &str = r#"TM Module

Jane Doe

   <jane@example.com>

   Copyright © 2003 Example
     __________________________________________________________________

   Table of Contents

   1. Admin Guide

        1. Overview
        3. Parameters

              3.1. fr_timer (integer)
              3.2. auto_inv_100_reason (string)

        4. Functions

              4.1. t_relay([host, port])
              4.2. t_newtran()

1. Overview

   Prose about the module. 3.1. looks like a heading but is indented
   in the ToC, and this line mentions Parameters casually.

3. Parameters

   3.1. fr_timer (integer)
   3.2. auto_inv_100_reason (string)

3.1. fr_timer (integer)

   This timer is used for all SIP requests. It hits if no reply
   arrives in time (F in milliseconds).

   Default value is 30000 ms (30 seconds).

   Example 1.1. Set fr_timer parameter
...
modparam("tm", "fr_timer", 10000)
...

3.2. auto_inv_100_reason (string)

   Doc with a path C:\x\y and trailing text.

4. Functions

   4.1. t_relay([host, port])

4.1.  t_relay([host, port])

   Relay a SIP request statefully to the destination in the current
   URI. Second sentence same paragraph.

   Second paragraph excluded.

4.2.  t_newtran()

   Creates a new transaction.

5. RPC Commands

5.1. tm.t_uac_start

   Not a script function; must not be collected.

Chapter 2. Developer Guide

1. Available Functions

1.1.  register_tmcb(cb_type, cb_func)

   C API, not a script function; must not be collected.
"#;

#[test]
fn parses_readme_params_and_functions() {
    let m = parse_readme_txt("tm", FIXTURE).expect("fixture parses");
    assert_eq!(m.name, "tm");
    let pnames: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(pnames, vec!["fr_timer", "auto_inv_100_reason"]);
    assert_eq!(m.params[0].detail, "integer");
    assert_eq!(
        m.params[0].doc,
        "This timer is used for all SIP requests. It hits if no reply arrives in time (F in milliseconds)."
    );
    assert!(m.params[1].doc.contains(r"C:\x\y"));

    let fnames: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(fnames, vec!["t_relay", "t_newtran"]);
    assert_eq!(m.functions[0].detail, "t_relay([host, port])");
    assert!(m.functions[0].doc.starts_with("Relay a SIP request"));
    assert!(!m.functions[0].doc.contains("Second paragraph excluded"));
}

#[test]
fn toc_lines_are_not_items() {
    // the indented ToC repeats every heading: nothing may be doubled
    let m = parse_readme_txt("tm", FIXTURE).unwrap();
    assert_eq!(m.params.len(), 2);
    assert_eq!(m.functions.len(), 2);
}

#[test]
fn rpc_and_devel_chapters_are_not_script_symbols() {
    let m = parse_readme_txt("tm", FIXTURE).unwrap();
    assert!(!m.functions.iter().any(|f| f.name == "register_tmcb"));
    assert!(!m.functions.iter().any(|f| f.name.contains("t_uac_start")));
}

#[test]
fn empty_and_nul_are_errors() {
    assert!(parse_readme_txt("m", "").is_err());
    assert!(parse_readme_txt("m", "a\0b").is_err());
}

#[test]
fn readme_without_export_chapters_is_ok_and_empty() {
    let m = parse_readme_txt("m", "M Module\n\n   Just prose.\n").unwrap();
    assert!(m.params.is_empty() && m.functions.is_empty());
}

#[test]
fn hostile_doc_content_is_sanitized_at_harvest() {
    let txt = r#"Evil Module

3. Parameters

3.1. bad_param (string)

   Text with <img src=x onerror=alert(1)> raw html, a
   [command link](command:workbench.action.evil), a
   [js link](javascript:alert(1)), and a
   [good link](https://example.com/doc) that stays.
"#;
    let m = parse_readme_txt("evil", txt).unwrap();
    let doc = &m.params[0].doc;
    assert!(
        !doc.contains('<') && !doc.contains('>'),
        "raw html must be stripped: {doc}"
    );
    assert!(
        !doc.contains("command:"),
        "command links neutralized: {doc}"
    );
    assert!(!doc.contains("javascript:"), "js links neutralized: {doc}");
    assert!(doc.contains("command link"), "link labels survive: {doc}");
    assert!(
        doc.contains("https://example.com/doc"),
        "http(s) links survive"
    );
}

#[test]
fn adversarial_inputs_do_not_panic() {
    for s in [
        "3. Parameters",
        "3. Parameters\n3.1.",
        "3. Parameters\n3.1. \n",
        "4. Functions\n4.1.  (\n",
        "3. Parameters\n999999999999999999999. x (y)\n",
        "\\",
        "3. Parameters\n3.1. p (t)\n   doc \\ with backslash\n",
    ] {
        let _ = parse_readme_txt("m", s);
    }
}
