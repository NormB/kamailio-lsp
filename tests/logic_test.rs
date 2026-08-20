use kamailio_lsp::catalog::{Item, ModuleDoc};
use kamailio_lsp::logic::{CompKind, completions, definition_of, hover_markdown};

fn catalog() -> Vec<ModuleDoc> {
    vec![
        ModuleDoc {
            name: "tm".into(),
            params: vec![Item {
                name: "fr_timer".into(),
                detail: "integer".into(),
                doc: "Final response timer.".into(),
            }],
            functions: vec![Item {
                name: "t_relay".into(),
                detail: "t_relay([host, port])".into(),
                doc: "Relay statefully.".into(),
            }],
        },
        ModuleDoc {
            name: "htable".into(),
            params: vec![Item {
                name: "htable".into(),
                detail: "string".into(),
                doc: "Hash table declaration.".into(),
            }],
            functions: vec![Item {
                name: "sht_lock".into(),
                detail: "sht_lock(htable=>key)".into(),
                doc: "Locks a slot.".into(),
            }],
        },
    ]
}

const DOC: &str = "loadmodule \"tm.so\"\n\nroute[RELAY] {\n    t_relay();\n}\nrequest_route {\n    route(RELAY);\n}\n";

#[test]
fn modparam_value_position_offers_params_of_that_module() {
    let items = completions(&catalog(), DOC, r#"modparam("htable", ""#);
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(names, vec!["htable"]);
    assert_eq!(items[0].kind, CompKind::Param);
}

#[test]
fn modparam_module_position_offers_module_names() {
    let items = completions(&catalog(), DOC, r#"modparam(""#);
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(names.contains(&"tm") && names.contains(&"htable"));
    assert!(items.iter().all(|i| i.kind == CompKind::Module));
}

#[test]
fn loadmodule_position_offers_so_names() {
    let items = completions(&catalog(), DOC, r#"loadmodule ""#);
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"tm.so"));
    assert!(labels.contains(&"htable.so"));
}

#[test]
fn code_position_offers_loaded_module_functions_and_routes() {
    let items = completions(&catalog(), DOC, "    t_re");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    // tm is loaded → t_relay offered; htable is NOT loaded → not offered
    assert!(labels.contains(&"t_relay"));
    assert!(!labels.contains(&"sht_lock"));
    // route names as targets
    assert!(labels.contains(&"RELAY"));
}

#[test]
fn code_position_offers_kamailio_keywords() {
    let items = completions(&catalog(), DOC, "    ");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    for kw in [
        "if", "else", "switch", "while", "exit", "drop", "return", "route",
    ] {
        assert!(labels.contains(&kw), "missing keyword {kw}: {labels:?}");
    }
    // opensips-only keywords must not be suggested
    for kw in ["async", "launch"] {
        assert!(!labels.contains(&kw), "opensips-only keyword {kw} offered");
    }
}

#[test]
fn hover_finds_function_param_and_module() {
    let h = hover_markdown(&catalog(), DOC, "t_relay").expect("function hover");
    assert!(h.contains("t_relay([host, port])"));
    assert!(h.contains("Relay statefully."));

    let h = hover_markdown(&catalog(), DOC, "fr_timer").expect("param hover");
    assert!(h.contains("integer") && h.contains("Final response timer."));

    let h = hover_markdown(&catalog(), DOC, "tm").expect("module hover");
    assert!(h.contains("tm"));

    assert!(hover_markdown(&catalog(), DOC, "no_such_thing").is_none());
}

#[test]
fn definition_resolves_route_reference() {
    // cursor on "RELAY" inside route(RELAY) on line 6
    let d = definition_of(DOC, 6, 11).expect("definition");
    assert_eq!(d.line, 2); // route[RELAY] { on line 2
    // cursor elsewhere → none
    assert!(definition_of(DOC, 0, 0).is_none());
    // out of range must not panic
    assert!(definition_of(DOC, 999, 999).is_none());
}

#[test]
fn adversarial_docs_do_not_panic() {
    for text in ["", "\0", "route(", "modparam(\"x", "#!define \\"] {
        let _ = completions(&catalog(), text, text);
        let _ = hover_markdown(&catalog(), text, "x");
        let _ = definition_of(text, 0, 0);
    }
}

fn core() -> kamailio_lsp::catalog::CoreDocs {
    kamailio_lsp::catalog::CoreDocs {
        functions: vec![Item {
            name: "force_rport".into(),
            detail: "force_rport()".into(),
            doc: "Forces rport handling.".into(),
        }],
        params: vec![Item {
            name: "advertised_address".into(),
            detail: "Core parameters".into(),
            doc: "Address advertised in Via.".into(),
        }],
        pvars: vec![Item {
            name: "$ru".into(),
            detail: "Request URI".into(),
            doc: "The full request URI.".into(),
        }],
    }
}

#[test]
fn code_position_offers_core_functions() {
    use kamailio_lsp::logic::completions_with_core;
    let items = completions_with_core(&catalog(), &core(), DOC, "    force_");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"force_rport"));
    assert!(labels.contains(&"t_relay")); // module functions still there
    assert!(labels.contains(&"advertised_address")); // core params too
}

#[test]
fn dollar_prefix_offers_pseudo_variables_only() {
    use kamailio_lsp::logic::completions_with_core;
    let items = completions_with_core(&catalog(), &core(), DOC, "    xlog(\"$");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, vec!["$ru"]);
}

#[test]
fn hover_covers_core_items() {
    use kamailio_lsp::logic::hover_markdown_with_core;
    let h = hover_markdown_with_core(&catalog(), &core(), DOC, "force_rport").unwrap();
    assert!(h.contains("force_rport()"));
    let h = hover_markdown_with_core(&catalog(), &core(), DOC, "ru").unwrap();
    assert!(h.contains("Request URI"));
    let h = hover_markdown_with_core(&catalog(), &core(), DOC, "advertised_address").unwrap();
    assert!(h.contains("Via"));
    // module symbols still win where they exist
    let h = hover_markdown_with_core(&catalog(), &core(), DOC, "t_relay").unwrap();
    assert!(h.contains("module tm"));
}

use kamailio_lsp::logic::{route_occurrences, route_symbol_at, valid_route_name};

#[test]
fn route_symbol_at_finds_refs_and_def_names() {
    let doc = "request_route {\n    route(to.b);\n}\nroute[to.b] {\n    exit;\n}\n";
    // on the ref name (line 1, "to.b" starts at byte col 10)
    assert_eq!(route_symbol_at(doc, 1, 10), Some("to.b".to_string()));
    assert_eq!(route_symbol_at(doc, 1, 13), Some("to.b".to_string()));
    // one past the name → no symbol
    assert_eq!(route_symbol_at(doc, 1, 14), None);
    // on the def name (line 3, "to.b" starts at byte col 6)
    assert_eq!(route_symbol_at(doc, 3, 6), Some("to.b".to_string()));
    // on the keyword, not the name
    assert_eq!(route_symbol_at(doc, 3, 0), None);
    // out of range must not panic
    assert_eq!(route_symbol_at(doc, 99, 99), None);
    assert_eq!(route_symbol_at("", 0, 0), None);
}

#[test]
fn event_route_names_with_colon_are_symbols() {
    let doc = "event_route[htable:mod-init] {\n    exit;\n}\n";
    // cursor inside the bracketed name (byte col 12 = "htable:mod-init")
    assert_eq!(
        route_symbol_at(doc, 0, 15),
        Some("htable:mod-init".to_string())
    );
    let occ = route_occurrences(doc, "htable:mod-init");
    assert_eq!(occ.len(), 1);
    assert!(occ[0].1, "the event_route name span is the definition");
}

#[test]
fn route_occurrences_cover_defs_and_refs() {
    let doc = "request_route {\n    route(A);\n    route(\"A\");\n}\nroute[A] {\n    route(A);\n}\nroute[B] { exit; }\n";
    let occ = route_occurrences(doc, "A");
    // 3 refs + 1 def
    assert_eq!(occ.len(), 4);
    assert_eq!(occ.iter().filter(|(_, is_def)| *is_def).count(), 1);
    let def = occ.iter().find(|(_, d)| *d).unwrap();
    assert_eq!((def.0.line, def.0.col), (4, 6));
    // no occurrences of an unknown name
    assert!(route_occurrences(doc, "ZZ").is_empty());
    // adversarial: the empty name never matches unnamed blocks
    assert!(route_occurrences(doc, "").is_empty());
}

#[test]
fn definition_resolves_dotted_route_names() {
    let doc = "request_route {\n    route(to.b);\n}\nroute[to.b] {\n    exit;\n}\n";
    // cursor on the dot: word_at-based matching used to split here
    let d = definition_of(doc, 1, 12).expect("definition of dotted name");
    assert_eq!(d.name, "to.b");
    assert_eq!(d.line, 3);
}

#[test]
fn valid_route_name_gate() {
    assert!(valid_route_name("RELAY"));
    // kamailio event-route names carry ':' and '-'
    assert!(valid_route_name("htable:mod-init"));
    assert!(valid_route_name("to.b_1:x-y"));
    assert!(!valid_route_name(""));
    assert!(!valid_route_name("has space"));
    assert!(!valid_route_name("quote\""));
    assert!(!valid_route_name("nul\0"));
    assert!(!valid_route_name("paren("));
    assert!(!valid_route_name("bracket]"));
    assert!(!valid_route_name("back\\slash"));
}
