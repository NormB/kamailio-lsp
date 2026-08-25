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

// nat_traversal/call_control style: "Exported parameters" with a
// lowercase p (and "Exported functions"/"Functions" mixtures)
const CASE_FIXTURE: &str = r#"NAT Module

4. Exported parameters

4.1. keepalive_interval (integer)

   How often keepalives are sent.

5. Exported functions

5.1.  nat_keepalive()

   Arms keepalive for the current registration.
"#;

#[test]
fn chapter_titles_match_case_insensitively() {
    let m = parse_readme_txt("nat_traversal", CASE_FIXTURE).expect("parses");
    assert_eq!(m.params.len(), 1, "lowercase 'parameters' chapter: {m:?}");
    assert_eq!(m.params[0].name, "keepalive_interval");
    assert_eq!(m.functions.len(), 1);
    assert_eq!(m.functions[0].name, "nat_keepalive");
}

// kafka/keepalive/lrkproxy/seas style: the admin guide is one
// numbered chapter, so Parameters/Functions nest one level deeper
// (`2.3. Parameters` with `2.3.1. brokers (string)` items)
const NESTED_FIXTURE: &str = r#"Kafka Module

2. Admin Guide

2.1. Overview

   Prose.

2.3. Parameters

2.3.1. brokers (string)

   Specifies a list of brokers separated by commas.

   More prose.

2.3.2. topic (string)

   Topic name.

2.4. Functions

2.4.1.  kafka_send(topic, msg)

   Sends a message. Second sentence.

2.4.1.1. Return value

   Sub-sub sections are neither items nor section resets.

2.4.2.  kafka_send_key(topic, msg, key)

   Sends with a key.

2.5. RPC Commands

2.5.1. kafka.stats

   Not a script function.
"#;

#[test]
fn nested_chapters_are_harvested_depth_relatively() {
    let m = parse_readme_txt("kafka", NESTED_FIXTURE).expect("parses");
    let pnames: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(pnames, vec!["brokers", "topic"], "{m:?}");
    assert_eq!(
        m.params[0].doc,
        "Specifies a list of brokers separated by commas."
    );
    let fnames: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(fnames, vec!["kafka_send", "kafka_send_key"]);
    assert!(m.functions[0].doc.starts_with("Sends a message."));
    // the sub-sub "Return value" section neither becomes an item nor
    // swallows the following real item
    assert!(!m.functions.iter().any(|f| f.name.contains("Return")));
    assert!(!m.functions.iter().any(|f| f.name.contains("stats")));
}

#[test]
fn nested_adversarial_do_not_panic() {
    for s in [
        "2.3. Parameters
2.3.1.",
        "2.3. Parameters
9.9.9. x (y)
",
        "2.3. parameters
2.3.1. p (t)
   doc
",
        "2.3. Parameters
2.3.1.1.1.1. deep (t)
",
        "999999999999999999999.1. Parameters
",
    ] {
        let _ = parse_readme_txt("m", s);
    }
}

#[test]
fn param_headings_without_a_space_before_the_type_parse() {
    // real shape from the presence/presence_xml READMEs:
    // `3.1. db_url(str)` — no space before the paren (found by the
    // catalog-vs-accepted-corpus probe, 2026-08-20)
    let txt = "Presence Module\n\n3. Parameters\n\n3.1. db_url(str)\n\n   Database URL.\n\n3.2. spaced (int)\n\n   Spaced form still works.\n";
    let m = parse_readme_txt("presence", txt).expect("parses");
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["db_url", "spaced"], "{m:?}");
    assert_eq!(m.params[0].detail, "str");
    assert_eq!(m.params[1].detail, "int");
}

/// A module whose parameters are GROUPED into sub-sections.
///
/// `kazoo` writes `4.1. amqp related` with the parameters themselves
/// at `4.1.1.`.  Reading the group headings as the items harvested
/// four entries no `modparam` could ever write and threw away all
/// thirty real parameters, so a configuration setting any of them was
/// told, in a warning, that the parameter does not exist.  The
/// function chapter is grouped the same way.
const GROUPED: &str = r#"Kazoo Module

4. Parameters

   4.1. amqp related

        4.1.1. node_hostname(str)

4.1. amqp related

4.1.1. node_hostname(str)

   The hostname to announce.

4.1.2. amqp_max_channels(str)

   How many channels.

4.2. presence related

4.2.1. db_url(str)

   Database URL.

5. Functions

5.1. publishing

5.1.1. kazoo_publish(exchange, routing_key, payload)

   Publish a message.
"#;

#[test]
fn grouped_parameters_are_harvested_from_the_group() {
    let m = parse_readme_txt("kazoo", GROUPED).expect("parses");
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["node_hostname", "amqp_max_channels", "db_url"],
        "{names:?}"
    );
    assert_eq!(m.params[0].detail, "str");
}

#[test]
fn a_group_heading_is_not_itself_a_parameter() {
    let m = parse_readme_txt("kazoo", GROUPED).unwrap();
    for bad in ["amqp related", "presence related"] {
        assert!(
            !m.params.iter().any(|p| p.name == bad),
            "the group heading `{bad}` was harvested as a parameter"
        );
    }
}

#[test]
fn grouped_functions_are_harvested_from_the_group() {
    let m = parse_readme_txt("kazoo", GROUPED).unwrap();
    let names: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["kazoo_publish"], "{names:?}");
}

/// A function's own sub-subsection is prose, not another function.
///
/// `seas` documents `3.1.1. Return value` under one of its functions.
/// A rule that read every deeper heading as the real item would drop
/// the function and harvest its prose heading instead — the grouping
/// above is recognised because the OUTER heading is not item-shaped
/// and the inner one is, which is the other way round here.
#[test]
fn a_sub_subsection_under_a_function_does_not_replace_it() {
    let txt = "Seas Module\n\n3. Functions\n\n3.1. as_relay_t(app_name)\n\n   Relay.\n\n3.1.1. Return value\n\n   Negative on error.\n";
    let m = parse_readme_txt("seas", txt).unwrap();
    let names: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["as_relay_t"], "{names:?}");
}

/// The type written with no parentheses at all.
///
/// `ims_qos` writes `3.14. terminate_dialog_on_rx_failure integer`,
/// so the type ended up inside the name and the entry could never
/// match the `modparam` it was supposed to document.  Only a word
/// that is actually a type counts: `3.1. crl` and
/// `3.2. script_counter` are real parameters documented with no type
/// at all, and `3.3. db_qtable and db_ctable` is prose.
#[test]
fn a_type_written_without_parentheses_is_not_part_of_the_name() {
    let txt = "IMS QoS Module\n\n3. Parameters\n\n3.1. terminate_dialog_on_rx_failure integer\n\n   Terminate it.\n\n3.2. crl\n\n   A parameter with no type at all.\n\n3.3. script_counter\n\n   Another.\n";
    let m = parse_readme_txt("ims_qos", txt).unwrap();
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["terminate_dialog_on_rx_failure", "crl", "script_counter"],
        "{names:?}"
    );
    assert_eq!(m.params[0].detail, "integer");
    assert_eq!(m.params[1].detail, "");
}

/// Parameters documented in a chapter of their own.
///
/// `carrierroute` and `matrix` put their database parameters under
/// `Chapter 2. Module parameter for database access.`, whose items
/// restart at `1.` and sit at the top level.  `Chapter N.` is not a
/// numbered heading, so the chapter was invisible and eleven
/// parameters went unharvested.
const PARAM_CHAPTER: &str = r#"Carrierroute Module

Chapter 1. Admin Guide

3. Parameters

3.1. subscriber_table (string)

   The table.

Chapter 2. Module parameter for database access.

   Table of Contents

   1. db_url (String)
   2. carrierroute_table (String)

1. db_url (String)

   The database URL.

2. carrierroute_table (String)

   The routing table.

Chapter 3. Developer Guide

1. Available Functions

1.1. not_a_script_function(x)

   C API.
"#;

#[test]
fn a_parameter_chapter_of_its_own_is_harvested() {
    let m = parse_readme_txt("carrierroute", PARAM_CHAPTER).expect("parses");
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["subscriber_table", "db_url", "carrierroute_table"],
        "{names:?}"
    );
    assert_eq!(m.params[1].detail, "String");
}

#[test]
fn the_next_chapter_ends_a_parameter_chapter() {
    let m = parse_readme_txt("carrierroute", PARAM_CHAPTER).unwrap();
    assert!(
        m.functions.is_empty(),
        "the developer guide leaked in: {:?}",
        m.functions
    );
    assert!(
        !m.params.iter().any(|p| p.name == "not_a_script_function"),
        "the developer guide leaked into the parameters"
    );
}

/// Parameters after the chapter's numbering restarts.
///
/// `rtpengine` documents 85 parameters as `5.1.`…`5.86.` and then,
/// for the last nine, drops back to `6.`, `7.`, … — an upstream
/// numbering glitch, but `ping_interval` and `enable_dmq` are real
/// parameters and every configuration setting one of them was warned
/// about it.  A heading at the chapter's own depth normally ENDS the
/// chapter, so what saves this from swallowing `15. Functions` is
/// that a renumbered item still carries its type and a chapter title
/// does not.
#[test]
fn parameters_after_the_numbering_restarts_are_harvested() {
    let txt = "RTPEngine Module\n\n5. Parameters\n\n5.1. rtpengine_sock (string)\n\n   The socket.\n\n6. ping_interval (integer)\n\n   Seconds between pings.\n\n7. enable_dmq (integer)\n\n   DMQ replication.\n\n15. Functions\n\n15.1. rtpengine_offer([flags])\n\n   Offer.\n\n16. Exported Pseudo Variables\n\n16.1. $rtpstat\n\n   Not a function.\n";
    let m = parse_readme_txt("rtpengine", txt).expect("parses");
    let params: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        params,
        vec!["rtpengine_sock", "ping_interval", "enable_dmq"],
        "{params:?}"
    );
    let funcs: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(funcs, vec!["rtpengine_offer"], "{funcs:?}");
}

#[test]
fn a_chapter_title_never_continues_the_parameter_chapter() {
    // the same shape with prose chapter titles: `4. Dependencies`
    // carries no type, so it ends the chapter the way it always did
    let txt = "M\n\n3. Parameters\n\n3.1. p (int)\n\n   Doc.\n\n4. Dependencies\n\n4.1. Kamailio Modules\n\n   Prose.\n";
    let m = parse_readme_txt("m", txt).unwrap();
    let params: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(params, vec!["p"], "{params:?}");
}
