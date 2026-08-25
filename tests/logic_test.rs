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
    let occ = ns_occurrences(doc, "htable:mod-init", &RouteNs::Kind("event_route".into()));
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
fn valid_route_name_gate_is_the_unquoted_id_charset() {
    // kamailio's unquoted route_name is ID = [A-Za-z_][A-Za-z0-9_]*
    // (cfg.lex; verified: route[a.b]/route[a:b]/route[a-b]/route[1ab]
    // are all rejected by the 6.0.1 and 6.1.4 binaries unless quoted) — rename
    // must only ever produce names that are legal WITHOUT quotes,
    // because definitions are commonly written unquoted
    assert!(valid_route_name("RELAY"));
    assert!(valid_route_name("_x1"));
    assert!(valid_route_name("a_b9"));
    assert!(!valid_route_name(""));
    assert!(!valid_route_name("1ab"), "leading digit is not an ID");
    assert!(!valid_route_name("a.b"), "dot needs quoting");
    assert!(!valid_route_name("a:b"), "colon is event-route grammar");
    assert!(!valid_route_name("a-b"), "dash needs quoting");
    assert!(!valid_route_name("htable:mod-init"));
    assert!(!valid_route_name("has space"));
    assert!(!valid_route_name("quote\""));
    assert!(!valid_route_name("nul\0"));
    assert!(!valid_route_name("paren("));
    assert!(!valid_route_name("bracket]"));
    assert!(!valid_route_name("back\\slash"));
}

use kamailio_lsp::logic::{completions_with_core, pvar_tail, signature_at};

fn sig_catalog() -> Vec<ModuleDoc> {
    vec![ModuleDoc {
        name: "tm".into(),
        params: vec![],
        functions: vec![Item {
            name: "t_relay".into(),
            detail: "t_relay([host, port])".into(),
            doc: "Relays the request.".into(),
        }],
    }]
}

#[test]
fn signature_at_finds_active_parameter() {
    let core = kamailio_lsp::catalog::CoreDocs::default();
    let doc = "loadmodule \"tm.so\"\nrequest_route {\n}\n";
    // first argument
    let s = signature_at(&sig_catalog(), &core, doc, "    t_relay(").expect("sig");
    assert_eq!(s.0, "t_relay([host, port])");
    assert_eq!(s.2, 0);
    // second argument (comma inside a string must not count)
    let s = signature_at(&sig_catalog(), &core, doc, r#"    t_relay("a,b", "#).expect("sig");
    assert_eq!(s.2, 1);
    // nested call: innermost unclosed wins
    let core_with_fn = kamailio_lsp::catalog::CoreDocs {
        functions: vec![Item {
            name: "xlog".into(),
            detail: "xlog([level], format)".into(),
            doc: String::new(),
        }],
        ..Default::default()
    };
    let s = signature_at(&sig_catalog(), &core_with_fn, doc, "    t_relay(xlog(").expect("sig");
    assert_eq!(s.0, "xlog([level], format)");
    // a CLOSED nested call pops back to the outer one
    let s = signature_at(
        &sig_catalog(),
        &core_with_fn,
        doc,
        "    t_relay(xlog(\"x\"), ",
    )
    .expect("sig");
    assert_eq!(s.0, "t_relay([host, port])");
    assert_eq!(s.2, 1);
    // unknown function → none
    assert!(signature_at(&sig_catalog(), &core, doc, "    nope(").is_none());
    // adversarial: never panic
    for p in ["", "(", ")))((", "\"", "t_relay(\"\\", "#t_relay(", "\0("] {
        let _ = signature_at(&sig_catalog(), &core, doc, p);
    }
}

#[test]
fn completions_dedup_prefers_richer_items() {
    // "exit" exists as a core KEYWORD and as a core function: one item
    // must survive, it must keep the documentation, and it must stay a
    // KEYWORD — `exit` is a statement (`exit;`), so completing it as a
    // `exit()` snippet would insert something the parser rejects.
    let core = kamailio_lsp::catalog::CoreDocs {
        functions: vec![Item {
            name: "exit".into(),
            detail: "exit".into(),
            doc: "Stops execution.".into(),
        }],
        ..Default::default()
    };
    let out = completions_with_core(&[], &core, "request_route {\n}\n", "    ");
    let exits: Vec<_> = out.iter().filter(|c| c.label == "exit").collect();
    assert_eq!(exits.len(), 1, "duplicate labels must collapse");
    assert_eq!(exits[0].kind, CompKind::Keyword);
    assert_eq!(
        exits[0].doc, "Stops execution.",
        "the documented entry must be the one that survives"
    );
}

#[test]
fn xlog_is_not_a_core_keyword() {
    // xlog is a MODULE function in kamailio (verified: unknown
    // command without loadmodule) — it must not be offered as a
    // keyword when nothing provides it
    let out = completions_with_core(
        &[],
        &kamailio_lsp::catalog::CoreDocs::default(),
        "request_route {\n}\n",
        "    ",
    );
    assert!(
        !out.iter().any(|c| c.label == "xlog"),
        "xlog offered with no module/catalog providing it"
    );
}

#[test]
fn paren_loadmodule_counts_for_function_completion() {
    let doc = "loadmodule(\"tm.so\")\nrequest_route {\n}\n";
    let out = completions(&catalog(), doc, "    t_re");
    assert!(
        out.iter().any(|c| c.label == "t_relay"),
        "loadmodule(paren) must load tm for completion"
    );
}

#[test]
fn route_call_argument_completes_route_names() {
    let doc = "route[RELAY] {\n    exit;\n}\nrequest_route {\n}\n";
    let out = completions_with_core(
        &[],
        &kamailio_lsp::catalog::CoreDocs::default(),
        doc,
        "    route(",
    );
    assert!(!out.is_empty());
    assert!(
        out.iter().all(|c| c.kind == CompKind::Route),
        "inside route( only route names complete: {:?}",
        out.iter().map(|c| &c.label).collect::<Vec<_>>()
    );
    assert!(out.iter().any(|c| c.label == "RELAY"));
    // quoted form and partial names too
    let out = completions_with_core(
        &[],
        &kamailio_lsp::catalog::CoreDocs::default(),
        doc,
        "    route(\"RE",
    );
    assert!(out.iter().any(|c| c.label == "RELAY"));
}

#[test]
fn pvar_tail_reports_replacement_length() {
    // "$ru" → replace "$ru" (3 bytes)
    assert_eq!(pvar_tail("    $ru"), Some(3));
    assert_eq!(pvar_tail("$"), Some(1));
    assert_eq!(pvar_tail("xlog($si"), Some(3));
    // not a pvar context
    assert_eq!(pvar_tail("xlog("), None);
    assert_eq!(pvar_tail(""), None);
    // "$x y" — space breaks the tail
    assert_eq!(pvar_tail("$x y"), None);
}

use kamailio_lsp::logic::{
    analyzer_diagnostics, include_closure, loaded_modules_multi, route_defs_multi,
};

#[test]
fn include_closure_follows_cycles_and_depth_safely() {
    use std::path::Path;
    // a.cfg includes b.cfg; b.cfg includes a.cfg (cycle) and missing.cfg
    let loader = |p: &Path| -> Option<String> {
        match p.to_str()? {
            "/x/a.cfg" => Some("include_file \"b.cfg\"\nroute[A_R] { exit; }\n".into()),
            "/x/b.cfg" => Some(
                "include_file \"a.cfg\"\ninclude_file \"missing.cfg\"\nloadmodule \"tm.so\"\nroute[B_R] { exit; }\n"
                    .into(),
            ),
            _ => None,
        }
    };
    let root_text = loader(Path::new("/x/a.cfg")).unwrap();
    let files = include_closure(Path::new("/x/a.cfg"), &root_text, &loader);
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.to_str().unwrap()).collect();
    assert_eq!(
        paths,
        vec!["/x/a.cfg", "/x/b.cfg"],
        "cycle visited once, missing skipped"
    );
    // multi-file views
    let mods = loaded_modules_multi(&files);
    assert_eq!(mods, vec!["tm"]);
    let defs = route_defs_multi(&files);
    let names: Vec<(&str, &str)> = defs
        .iter()
        .map(|(p, l)| (p.to_str().unwrap(), l.name.as_str()))
        .collect();
    assert!(names.contains(&("/x/a.cfg", "A_R")));
    assert!(names.contains(&("/x/b.cfg", "B_R")));
    // depth bomb: a chain of self-includes must terminate
    let bomb = |_: &Path| -> Option<String> { Some("include_file \"z.cfg\"\n".into()) };
    let files = include_closure(Path::new("/x/z.cfg"), "include_file \"z.cfg\"\n", &bomb);
    assert_eq!(files.len(), 1);
}

#[test]
fn analyzer_diagnostics_flag_undefined_and_duplicate_routes() {
    use std::path::Path;
    let loader = |_: &Path| -> Option<String> { None };
    // undefined route ref
    let text = "request_route {\n    route(NOPE);\n}\n";
    let ds = analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(ds[0].message.contains("NOPE"));
    assert_eq!(ds[0].line, 1);
    assert!(ds[0].col_start < ds[0].col_end);
    // defined in an include → clean
    let loader2 = |p: &Path| -> Option<String> {
        (p.to_str() == Some("/x/inc.cfg")).then(|| "route[NOPE] { exit; }\n".to_string())
    };
    let text2 = "include_file \"inc.cfg\"\nrequest_route {\n    route(NOPE);\n}\n";
    assert!(analyzer_diagnostics(Path::new("/x/t.cfg"), text2, &loader2).is_empty());
    // duplicate definitions: the LATER one in this file is flagged
    let text3 = "route[DUP] { exit; }\nroute[DUP] { exit; }\n";
    let ds = analyzer_diagnostics(Path::new("/x/t.cfg"), text3, &loader);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(ds[0].line, 1);
    assert!(ds[0].message.contains("DUP"));
    // unnamed blocks never collide (request_route + reply_route + route)
    let text4 = "request_route { exit; }\nreply_route { exit; }\nonsend_route { exit; }\n";
    assert!(analyzer_diagnostics(Path::new("/x/t.cfg"), text4, &loader).is_empty());
    // clean file → empty; adversarial → no panic
    assert!(
        analyzer_diagnostics(Path::new("/x/t.cfg"), "request_route { exit; }\n", &loader)
            .is_empty()
    );
    for s in [
        "",
        "\0",
        "route(",
        "include_file \"\0\"",
        "route(x) route[x]{}",
        "request_route { route(\\ ); }",
    ] {
        let _ = analyzer_diagnostics(Path::new("/x/t.cfg"), s, &loader);
    }
}

#[test]
fn include_diags_remap_to_the_include_directive() {
    use kamailio_lsp::diag::{Diag, Severity};
    use kamailio_lsp::logic::remap_include_diag;
    let checked = std::path::Path::new("/w/main.cfg");
    let root_text = "#!KAMAILIO\ninclude_file \"incdir/sub_bad.cfg\"\nrequest_route { exit; }\n";
    let mk = |file: &str| Diag {
        file: file.into(),
        line: 1,
        end_line: 1,
        col_start: 15,
        col_end: 16,
        severity: Severity::Error,
        message: "syntax error".into(),
    };
    // a diag for the checked file itself passes through unchanged
    let d = remap_include_diag(checked, root_text, &mk("/w/main.cfg")).expect("own diag");
    assert_eq!(d.line, 1);
    assert_eq!(d.message, "syntax error");
    // kamailio echoes the include path AS WRITTEN (relative): the diag
    // must attach to the include_file directive in the root
    let d = remap_include_diag(checked, root_text, &mk("incdir/sub_bad.cfg"))
        .expect("include diag must not be dropped");
    assert_eq!(d.line, 1, "attach at the include_file line");
    assert!(
        d.message.contains("incdir/sub_bad.cfg") && d.message.contains("line 2"),
        "message names the include and the real line: {}",
        d.message
    );
    assert!(d.message.contains("syntax error"));
    // absolute spelling of the same include target also attaches
    let d = remap_include_diag(checked, root_text, &mk("/w/incdir/sub_bad.cfg"))
        .expect("absolute include diag");
    assert_eq!(d.line, 1);
    // an unrelated file stays dropped (caller falls back on rc!=0)
    assert!(remap_include_diag(checked, root_text, &mk("/elsewhere/other.cfg")).is_none());
    // adversarial: never panic
    for f in ["", "\0", "..", "a\\b", "incdir/../incdir/sub_bad.cfg"] {
        let _ = remap_include_diag(checked, root_text, &mk(f));
    }
}

use kamailio_lsp::logic::IncludeGraph;

/// Configs as the workspace scan hands them over.
fn ws(entries: &[(&str, &str)]) -> Vec<(std::path::PathBuf, String)> {
    entries
        .iter()
        .map(|(p, t)| (std::path::PathBuf::from(p), t.to_string()))
        .collect()
}

#[test]
fn the_include_graph_finds_the_root_a_fragment_belongs_to() {
    use std::path::{Path, PathBuf};
    // Transitivity is free: `sub/auth.cfg` is never named by
    // kamailio.cfg, but routes.cfg is itself a scanned config and
    // names it, so walking parents reaches the top of the chain.
    let g = IncludeGraph::build(&ws(&[
        ("/w/kamailio.cfg", "include_file \"routes.cfg\"\n"),
        ("/w/routes.cfg", "import_file \"sub/auth.cfg\"\n"),
        ("/w/sub/auth.cfg", "route[AUTH] { exit; }\n"),
        ("/w/other.cfg", "request_route { exit; }\n"),
    ]));
    assert_eq!(
        g.analysis_root(Path::new("/w/sub/auth.cfg")),
        Some(PathBuf::from("/w/kamailio.cfg")),
        "a fragment two levels down belongs to the top of its chain"
    );
    assert_eq!(
        g.analysis_root(Path::new("/w/routes.cfg")),
        Some(PathBuf::from("/w/kamailio.cfg"))
    );
    // a config nothing includes is a program in its own right
    assert_eq!(g.analysis_root(Path::new("/w/kamailio.cfg")), None);
    assert_eq!(g.analysis_root(Path::new("/w/other.cfg")), None);
    // a file the scan never saw
    assert_eq!(g.analysis_root(Path::new("/w/nope.cfg")), None);
}

#[test]
fn the_include_graph_terminates_on_cycles_and_chooses_one_root_stably() {
    use std::path::{Path, PathBuf};
    // a <-> b: walking up from either must stop instead of looping
    let g = IncludeGraph::build(&ws(&[
        ("/w/a.cfg", "include_file \"b.cfg\"\n"),
        ("/w/b.cfg", "include_file \"a.cfg\"\n"),
    ]));
    let r = g.analysis_root(Path::new("/w/b.cfg"));
    assert!(r.is_some(), "a cycle still yields a decision: {r:?}");
    // self-include
    let g = IncludeGraph::build(&ws(&[("/w/z.cfg", "include_file \"z.cfg\"\n")]));
    assert_eq!(g.analysis_root(Path::new("/w/z.cfg")), None);
    // two roots include the same fragment: whichever is chosen, the
    // choice must not depend on scan order, or a fragment's context
    // would flicker between edits
    let fwd = IncludeGraph::build(&ws(&[
        ("/w/one.cfg", "include_file \"shared.cfg\"\n"),
        ("/w/two.cfg", "include_file \"shared.cfg\"\n"),
        ("/w/shared.cfg", "route[S] { exit; }\n"),
    ]));
    let rev = IncludeGraph::build(&ws(&[
        ("/w/shared.cfg", "route[S] { exit; }\n"),
        ("/w/two.cfg", "include_file \"shared.cfg\"\n"),
        ("/w/one.cfg", "include_file \"shared.cfg\"\n"),
    ]));
    let a = fwd.analysis_root(Path::new("/w/shared.cfg"));
    assert_eq!(a, rev.analysis_root(Path::new("/w/shared.cfg")));
    assert_eq!(a, Some(PathBuf::from("/w/one.cfg")));
    // adversarial: never panic
    for t in [
        "",
        "\0",
        "include_file",
        "include_file \"\0\"",
        "include_file \"..\"",
    ] {
        let g = IncludeGraph::build(&ws(&[("/w/x.cfg", t)]));
        let _ = g.analysis_root(Path::new("/w/x.cfg"));
    }
}

#[test]
fn a_fragment_is_analysed_in_its_roots_closure_not_on_its_own() {
    use kamailio_lsp::logic::{analyzer_diagnostics_in_closure, include_closure};
    use std::path::Path;
    // main.cfg defines HELPER and includes inc.cfg, which calls it.
    // Checked on its own inc.cfg looks broken; in the root's closure
    // it is not — the exact noise that made fragments unusable.
    let main_text = "include_file \"inc.cfg\"\nroute[HELPER] { exit; }\n";
    let inc_text = "request_route {\n    route(HELPER);\n}\n";
    let loader = |p: &Path| -> Option<String> {
        match p.to_str()? {
            "/w/main.cfg" => Some(main_text.to_string()),
            "/w/inc.cfg" => Some(inc_text.to_string()),
            _ => None,
        }
    };
    let alone = include_closure(Path::new("/w/inc.cfg"), inc_text, &loader);
    let ds = analyzer_diagnostics_in_closure(&alone, Path::new("/w/inc.cfg"), inc_text);
    assert_eq!(ds.len(), 1, "on its own the call is undefined: {ds:?}");

    let via_root = include_closure(Path::new("/w/main.cfg"), main_text, &loader);
    let ds = analyzer_diagnostics_in_closure(&via_root, Path::new("/w/inc.cfg"), inc_text);
    assert!(ds.is_empty(), "in the root's closure it is defined: {ds:?}");
    // positions still belong to the reported file only
    let ds = analyzer_diagnostics_in_closure(&via_root, Path::new("/w/main.cfg"), main_text);
    assert!(ds.is_empty(), "{ds:?}");
}

#[test]
fn a_fragments_check_diagnostics_are_routed_to_the_fragment() {
    use kamailio_lsp::diag::{Diag, Severity};
    use kamailio_lsp::logic::fragment_check_diag;
    use std::path::Path;
    // `kamailio -c` ran on the ROOT (cwd = its directory); the buffer
    // on screen is the fragment.
    let checked = Path::new("/w/main.cfg");
    let reported = Path::new("/w/incdir/sub.cfg");
    let mk = |file: &str| Diag {
        file: file.into(),
        line: 7,
        end_line: 7,
        col_start: 4,
        col_end: 9,
        severity: Severity::Error,
        message: "syntax error".into(),
    };
    // relative, as the checker spells an include
    let d = fragment_check_diag(checked, reported, &mk("incdir/sub.cfg")).expect("fragment diag");
    assert_eq!(d.line, 7, "at the fragment's own line, not folded");
    assert_eq!(d.col_start, 4);
    assert_eq!(d.message, "syntax error", "no include-directive prefix");
    assert_eq!(d.file, reported.display().to_string());
    // absolute spelling of the same file
    assert!(fragment_check_diag(checked, reported, &mk("/w/incdir/sub.cfg")).is_some());
    // the root's own errors belong to the root's buffer, not here
    assert!(fragment_check_diag(checked, reported, &mk("/w/main.cfg")).is_none());
    // a sibling fragment's errors are not this fragment's
    assert!(fragment_check_diag(checked, reported, &mk("incdir/other.cfg")).is_none());
    // an unpositioned line must not silently become the fragment's
    assert!(fragment_check_diag(checked, reported, &mk("")).is_none());
    // adversarial: never panic
    for f in ["\0", "..", "a\\b", "incdir/../incdir/sub.cfg"] {
        let _ = fragment_check_diag(checked, reported, &mk(f));
    }
}

use kamailio_lsp::logic::{RouteNs, ns_occurrences, route_symbol_ns_at};

#[test]
fn route_call_sites_are_not_satisfied_by_other_kinds() {
    // kamailio keeps per-kind route tables: route(X) invokes only the
    // main table (route[X]); a failure_route[X] is armed via
    // t_on_failure and lives elsewhere (cross-kind same-name configs
    // are legal, rc=0-verified)
    use std::path::Path;
    let loader = |_: &Path| -> Option<String> { None };
    let text = "failure_route[FOO] { exit; }\nrequest_route {\n    route(FOO);\n}\n";
    let ds = kamailio_lsp::logic::analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader);
    assert_eq!(
        ds.len(),
        1,
        "failure_route must not satisfy route(): {ds:?}"
    );
    assert!(ds[0].message.contains("FOO"));
    // a real route[FOO] does satisfy it
    let text2 = "route[FOO] { exit; }\nrequest_route {\n    route(FOO);\n}\n";
    assert!(
        kamailio_lsp::logic::analyzer_diagnostics(Path::new("/x/t.cfg"), text2, &loader).is_empty()
    );
}

#[test]
fn duplicates_are_tracked_per_kind() {
    use std::path::Path;
    let loader = |_: &Path| -> Option<String> { None };
    // same name across kinds is legal — no duplicate warning
    let text = "route[X] { exit; }\nfailure_route[X] { exit; }\nbranch_route[X] { exit; }\n";
    assert!(
        kamailio_lsp::logic::analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader).is_empty(),
        "cross-kind same-name is legal kamailio"
    );
    // same kind twice still warns
    let text2 = "failure_route[X] { exit; }\nfailure_route[X] { drop; }\n";
    let ds = kamailio_lsp::logic::analyzer_diagnostics(Path::new("/x/t.cfg"), text2, &loader);
    assert_eq!(ds.len(), 1, "{ds:?}");
}

#[test]
fn numeric_route_refs_are_dynamic_and_never_warn() {
    // route(N) builds a runtime rval expression (cfg.y ROUTE LPAREN
    // rval_expr) — index/name dispatch happens at runtime, so the
    // analyzer must stay quiet even without a route[N] block
    use std::path::Path;
    let loader = |_: &Path| -> Option<String> { None };
    let text = "request_route {\n    route(0);\n    route(7);\n}\n";
    assert!(
        kamailio_lsp::logic::analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader).is_empty(),
        "numeric refs are runtime-dispatched"
    );
}

#[test]
fn route_completion_offers_only_callable_names() {
    let doc = "route[CALLABLE] { exit; }\nfailure_route[ARMED] { exit; }\nevent_route[htable:mod-init] { exit; }\nrequest_route {\n}\n";
    let out = completions_with_core(
        &[],
        &kamailio_lsp::catalog::CoreDocs::default(),
        doc,
        "    route(",
    );
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    assert!(labels.contains(&"CALLABLE"));
    assert!(
        !labels.contains(&"ARMED"),
        "failure_route names are not route() targets: {labels:?}"
    );
    assert!(!labels.iter().any(|l| l.contains(':')));
}

#[test]
fn symbol_namespaces_separate_calls_from_other_kind_defs() {
    let doc = "route[X] { exit; }\nfailure_route[X] { drop; }\nrequest_route {\n    route(X);\n}\n";
    // on the call site: main namespace
    let (name, ns) = route_symbol_ns_at(doc, 3, 10).expect("call symbol");
    assert_eq!(name, "X");
    assert!(matches!(ns, RouteNs::Main));
    let occ = ns_occurrences(doc, &name, &ns);
    // route[X] def + route(X) call — NOT the failure_route[X] def
    assert_eq!(occ.len(), 2, "{occ:?}");
    assert_eq!(occ.iter().filter(|(_, d)| *d).count(), 1);
    assert!(occ.iter().all(|(l, _)| l.line != 1));
    // on the failure_route def name: its own namespace, only itself
    let (name, ns) = route_symbol_ns_at(doc, 1, 14).expect("failure def symbol");
    assert_eq!(name, "X");
    assert!(matches!(ns, RouteNs::Kind(ref k) if k == "failure_route"));
    let occ = ns_occurrences(doc, &name, &ns);
    assert_eq!(occ.len(), 1, "{occ:?}");
    assert_eq!(occ[0].0.line, 1);
    // definition_of from the call resolves to route[X], never the
    // failure_route
    let d = definition_of(doc, 3, 10).expect("definition");
    assert_eq!(d.line, 0);
}

#[test]
fn split_params_is_depth_and_quote_aware() {
    use kamailio_lsp::logic::split_params;
    // real kamailio signature: the optional pair is ONE parameter
    assert_eq!(split_params("t_relay([host, port])"), vec!["[host, port]"]);
    // nested calls, quoted commas, bracket groups
    assert_eq!(
        split_params("f(a, g(b, c), \"x,y\", [d, e])"),
        vec!["a", "g(b, c)", "\"x,y\"", "[d, e]"]
    );
    // single-quoted strings hide commas too
    assert_eq!(split_params("f('a,b', c)"), vec!["'a,b'", "c"]);
    assert_eq!(split_params("exit"), Vec::<String>::new());
    assert_eq!(split_params("f()"), Vec::<String>::new());
    // adversarial: never panic
    for s in ["", "(", ")", "f(((", "f(\"", "f('", "f(]) ", "f(a,,b)"] {
        let _ = split_params(s);
    }
}

use kamailio_lsp::logic::{
    SemKind, catalog_diagnostics, encode_semantic_tokens, quick_fixes, semantic_spans,
};

#[test]
fn quick_fixes_offer_loadmodule_and_route_stub() {
    // kamailio's message names NO function ("unknown command, missing
    // loadmodule?" — captured live 2026-08-20, position at the call's
    // closing paren), so the function is read from the document at
    // the diagnostic position
    let cat = sig_catalog(); // tm exports t_relay
    let doc = "loadmodule \"sl.so\"\nrequest_route {\n    t_relay();\n}\n";
    // diagnostic at line 2, 0-based col 12 (the `)`; 1-based column 13
    // in the capture)
    let fixes = quick_fixes(&cat, doc, "unknown command, missing loadmodule?", 2, 12);
    assert_eq!(fixes.len(), 1, "{fixes:?}");
    assert!(fixes[0].title.contains("tm"), "{}", fixes[0].title);
    assert_eq!(
        (fixes[0].line, fixes[0].col),
        (1, 0),
        "after the last loadmodule"
    );
    assert_eq!(fixes[0].insert, "loadmodule \"tm.so\"\n");
    // module already loaded → no fix
    let doc2 = "loadmodule \"tm.so\"\nrequest_route {\n    t_relay();\n}\n";
    assert!(quick_fixes(&cat, doc2, "unknown command, missing loadmodule?", 2, 12).is_empty());
    // paren load form still counts as "loaded" and as insert anchor
    let doc3 = "loadmodule(\"tm.so\")\nrequest_route {\n    t_relay();\n}\n";
    assert!(quick_fixes(&cat, doc3, "unknown command, missing loadmodule?", 2, 12).is_empty());
    // no loadmodule lines at all → insert at the top
    let fixes = quick_fixes(
        &cat,
        "request_route {\n    t_relay();\n}\n",
        "unknown command, missing loadmodule?",
        1,
        12,
    );
    assert_eq!((fixes[0].line, fixes[0].col), (0, 0));
    // a function nobody exports → no fix
    let fixes = quick_fixes(
        &cat,
        "request_route {\n    nosuch_fn(\"a\");\n}\n",
        "unknown command, missing loadmodule?",
        1,
        17,
    );
    assert!(fixes.is_empty(), "{fixes:?}");

    // undefined route → create a stub at end of file (exit; body —
    // empty route bodies are not valid kamailio)
    let doc4 = "request_route {\n    route(MISSING);\n}\n";
    let fixes = quick_fixes(
        &[],
        doc4,
        "route 'MISSING' is not defined here or in included files",
        1,
        10,
    );
    assert_eq!(fixes.len(), 1);
    assert!(fixes[0].title.contains("MISSING"));
    assert_eq!(fixes[0].line, 3, "appended at end of file");
    assert!(fixes[0].insert.contains("route[MISSING]"));
    assert!(fixes[0].insert.contains("exit;"));
    // adversarial messages/positions → no panic, no bogus fix
    for (m, l, c) in [
        ("", 0, 0),
        ("unknown command, missing loadmodule?", 99, 99),
        ("route '' is not defined", 0, 0),
        ("unknown command, missing loadmodule?", 0, 0),
    ] {
        let _ = quick_fixes(&cat, doc, m, l, c);
    }
}

#[test]
fn catalog_diagnostics_flag_undocumented_modparams() {
    let cat = catalog(); // tm documents fr_timer; htable documents htable
    // unknown param of a KNOWN module → warning at the param
    let text = "modparam(\"tm\", \"fr_tmer\", 5)\n";
    let origin = kamailio_lsp::catalog::CatalogOrigin::BuiltIn("6.1.4".to_string());
    let ds = catalog_diagnostics(&cat, &origin, text);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(ds[0].message.contains("fr_tmer") && ds[0].message.contains("tm"));
    assert_eq!(ds[0].line, 0);
    assert!(ds[0].col_start < ds[0].col_end);
    // documented param → clean
    assert!(catalog_diagnostics(&cat, &origin, "modparam(\"tm\", \"fr_timer\", 5)\n").is_empty());
    // UNKNOWN module → silent (the catalog may simply not cover it)
    assert!(catalog_diagnostics(&cat, &origin, "modparam(\"nope\", \"x\", 1)\n").is_empty());
    // empty catalog → silent everywhere
    assert!(catalog_diagnostics(&[], &origin, text).is_empty());
    // modparamx counts
    let ds = catalog_diagnostics(&cat, &origin, "modparamx(\"tm\", \"fr_tmer\", 5)\n");
    assert_eq!(ds.len(), 1);
}

#[test]
fn semantic_spans_cover_routes_and_pvars() {
    // pvars interpolate in BOTH quote styles: cfg.lex STRING1 and
    // STRING2 both return the same STRING token (cfg.lex:1309-1316),
    // and module fixups interpolate the string value afterwards
    let text = "route[RELAY] {\n    xlog(\"$ru\");\n    xlog('$rU');\n    $var(x) = 1;\n}\nrequest_route {\n    route(RELAY);\n}\n";
    let spans = semantic_spans(text);
    let routes: Vec<_> = spans
        .iter()
        .filter(|s| s.kind == SemKind::RouteName)
        .collect();
    assert_eq!(routes.len(), 2, "{spans:?}");
    assert_eq!((routes[0].line, routes[0].col, routes[0].len), (0, 6, 5));
    assert_eq!((routes[1].line, routes[1].col, routes[1].len), (6, 10, 5));
    let pvars: Vec<_> = spans.iter().filter(|s| s.kind == SemKind::Pvar).collect();
    assert!(
        pvars.iter().any(|p| p.line == 1),
        "$ru inside the double-quoted string"
    );
    assert!(
        pvars.iter().any(|p| p.line == 2),
        "$rU inside the single-quoted string"
    );
    assert!(pvars.iter().any(|p| p.line == 3 && p.len == 7), "$var(x)");
    // comments and directives contribute nothing
    assert!(semantic_spans("# $ru route(x)\n").is_empty());
    assert!(semantic_spans("// $ru\n").is_empty());
    assert!(semantic_spans("#!define X $ru \\\\\n  $rd\n").is_empty());
    // a '#' inside a string does not comment out the rest of the line
    let s2 = semantic_spans("request_route { xlog(\"#\"); xlog(\"$ru\"); }\n");
    assert!(
        s2.iter().any(|s| s.kind == SemKind::Pvar),
        "string '#' must not eat the line: {s2:?}"
    );
    // adversarial: no panic
    for s in ["", "$", "$(", "route[", "\0$ru"] {
        let _ = semantic_spans(s);
    }
}

#[test]
fn semantic_tokens_delta_encoding() {
    let text = "route[AB] {\n    route(AB);\n}\n";
    let data = encode_semantic_tokens(text);
    // LSP quintuples: deltaLine, deltaStart, length, tokenType, mods
    assert_eq!(data.len() % 5, 0);
    assert!(!data.is_empty());
    // first token: line 0, col 6, len 2, type 0 (route name)
    assert_eq!(&data[..5], &[0, 6, 2, 0, 0]);
    // second token on line 1 → deltaLine 1, absolute col 10
    assert_eq!(&data[5..10], &[1, 10, 2, 0, 0]);
}

#[test]
fn semantic_tokens_range_filters_and_reencodes() {
    use kamailio_lsp::logic::encode_semantic_tokens_range;
    // three routes over five lines; the middle slice covers only B
    let text =
        "route[A] {\n    exit;\n}\nroute[B] {\n    route(B);\n}\nroute[C] {\n    route(A);\n}\n";
    // range covering lines 3..=5 only
    let data = encode_semantic_tokens_range(text, (3, 0), (6, 0));
    assert_eq!(data.len(), 10, "exactly B's def and call: {data:?}");
    // first token restarts the delta chain at the DOCUMENT origin:
    // deltaLine is the absolute line of the first in-range token
    assert_eq!(&data[..5], &[3, 6, 1, 0, 0]);
    assert_eq!(&data[5..10], &[1, 10, 1, 0, 0]);
    // a range starting mid-line excludes spans before its column
    let data = encode_semantic_tokens_range(text, (3, 7), (6, 0));
    assert_eq!(
        &data[..5],
        &[4, 10, 1, 0, 0],
        "B's def starts before the range: {data:?}"
    );
    // empty range yields nothing; inverted/absurd ranges do not panic
    assert!(encode_semantic_tokens_range(text, (4, 0), (4, 0)).is_empty());
    let _ = encode_semantic_tokens_range(text, (99, 0), (0, 0));
    let _ = encode_semantic_tokens_range("", (0, 0), (99, 99));
    let _ = encode_semantic_tokens_range("\u{0}route[A]{}", (0, 0), (9, 9));
    // whole-document range equals the full encoding
    assert_eq!(
        encode_semantic_tokens_range(text, (0, 0), (999, 0)),
        kamailio_lsp::logic::encode_semantic_tokens(text)
    );
}

/// A route reached through a `#!define` alias is not undefined — the
/// preprocessor expands it before the parser ever sees it.  All three
/// shapes below are accepted by the 6.1.4 binary (rc=0), so warning on
/// any of them is the analyzer being wrong in the user's face.
#[test]
fn a_route_reached_through_a_define_is_not_undefined() {
    use std::path::Path;
    let loader = |_: &Path| -> Option<String> { None };

    // expands to a number: route(1) dispatches by index
    let text = "#!define RELAY 1\nrequest_route {\n    route(RELAY);\n}\nroute[1] { exit; }\n";
    assert!(
        analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader).is_empty(),
        "{:?}",
        analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader)
    );

    // expands to another route's name
    let text =
        "#!define RELAY MYROUTE\nrequest_route {\n    route(RELAY);\n}\nroute[MYROUTE] { exit; }\n";
    assert!(
        analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader).is_empty(),
        "{:?}",
        analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader)
    );

    // a bare define has no value; the expansion is `route()`, which the
    // real parser rejects — not the analyzer's call to make
    let text = "#!define RELAY\nrequest_route {\n    route(RELAY);\n}\n";
    assert!(analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader).is_empty());

    // a define from an included file counts too
    let loader2 = |p: &Path| -> Option<String> {
        (p.to_str() == Some("/x/inc.cfg")).then(|| "#!define RELAY MYROUTE\n".to_string())
    };
    let text = "include_file \"inc.cfg\"\nrequest_route {\n    route(RELAY);\n}\nroute[MYROUTE] { exit; }\n";
    assert!(
        analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader2).is_empty(),
        "a define in an include is still a define"
    );
}

/// Resolving through a define must not become a way to hide a real
/// mistake: if the expansion names no route either, that is still an
/// undefined route, and the message says which name was checked.
#[test]
fn a_define_expanding_to_nothing_real_still_warns() {
    use std::path::Path;
    let loader = |_: &Path| -> Option<String> { None };
    let text = "#!define RELAY GHOST\nrequest_route {\n    route(RELAY);\n}\n";
    let ds = analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(
        ds[0].message.contains("RELAY") && ds[0].message.contains("GHOST"),
        "the message must name both the alias and what it expands to: {}",
        ds[0].message
    );
    // and it points at the call site, not the define
    assert_eq!(ds[0].line, 2);
}

/// A define chain must terminate even when it is circular.
#[test]
fn a_circular_define_chain_terminates() {
    use std::path::Path;
    let loader = |_: &Path| -> Option<String> { None };
    let text = "#!define A B\n#!define B A\nrequest_route {\n    route(A);\n}\n";
    let _ = analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader);
}

#[test]
fn the_check_failure_note_never_invents_a_file_or_a_line() {
    use kamailio_lsp::diag::{Diag, Severity};
    use kamailio_lsp::logic::check_failure_note;
    let mk = |file: &str| Diag {
        file: file.into(),
        line: 0,
        end_line: 0,
        col_start: 0,
        col_end: 1,
        severity: Severity::Error,
        message: "no transport protocol loaded".into(),
    };
    // positioned: the note says where, so the reader can go there
    let n = check_failure_note(Some(&mk("/w/root.cfg")), 1);
    assert!(n.contains("/w/root.cfg") && n.contains("line 1"), "{n}");
    // NOT positioned — a missing module, a bad module path.  Naming a
    // file and a line the parser never gave renders as
    // "check failed in , line 1: ..." and sends the reader to a line
    // that has nothing to do with it.
    let n = check_failure_note(Some(&mk("")), 1);
    assert!(!n.contains(" in ,"), "invented an empty file: {n}");
    assert!(!n.contains("line 1"), "invented a line: {n}");
    assert!(n.contains("no transport protocol loaded"), "{n}");
    // nothing parsed at all: the exit status is all there is
    let n = check_failure_note(None, 255);
    assert!(n.contains("255"), "{n}");
    assert!(!n.contains("line"), "{n}");
}

#[test]
fn the_include_graph_folds_a_path_that_climbs_out_of_its_directory() {
    use kamailio_lsp::logic::IncludeGraph;
    use std::path::{Path, PathBuf};
    // A per-site split: `sites/site-a.cfg` reaches shared routing
    // through `../common/`, which is the only spelling the directive
    // CAN use.  The editor opens `/w/common/routing.cfg`; the
    // directive resolves to `/w/sites/../common/routing.cfg`.  One
    // file, two names — keyed apart, the fragment has no root and the
    // whole feature silently does nothing for that layout.
    let g = IncludeGraph::build(&ws(&[
        (
            "/w/sites/site-a.cfg",
            "include_file \"../common/routing.cfg\"\n",
        ),
        ("/w/common/routing.cfg", "route[R] { exit; }\n"),
    ]));
    assert_eq!(
        g.analysis_root(Path::new("/w/common/routing.cfg")),
        Some(PathBuf::from("/w/sites/site-a.cfg")),
        "a sibling directory reached through .. is the same file"
    );
    // `..` at the filesystem root is the root (POSIX), not a level above it
    let g = IncludeGraph::build(&ws(&[("/a.cfg", "include_file \"../x.cfg\"\n")]));
    assert_eq!(
        g.analysis_root(Path::new("/x.cfg")),
        Some(PathBuf::from("/a.cfg"))
    );
    // a `..` with nothing to fold into is kept rather than dropped,
    // or the path would name a different file entirely
    let g = IncludeGraph::build(&ws(&[("rel.cfg", "include_file \"../up.cfg\"\n")]));
    assert_eq!(
        g.analysis_root(Path::new("../up.cfg")),
        Some(PathBuf::from("rel.cfg"))
    );
}

#[test]
fn the_closure_visits_a_doubly_named_include_once() {
    use kamailio_lsp::logic::include_closure;
    use std::path::Path;
    // The same file named twice — once directly, once through a
    // round trip — must not appear twice, or every route it defines
    // reads as "defined more than once".
    // the loader resolves BOTH spellings, as the filesystem does —
    // otherwise the second is skipped for being unloadable and the
    // test proves nothing
    let loader = |p: &Path| -> Option<String> {
        let resolved = p.to_str()?.replace("/inc/../inc/", "/inc/");
        match resolved.as_str() {
            "/w/kamailio.cfg" => Some(
                "include_file \"inc/routes.cfg\"\ninclude_file \"inc/../inc/routes.cfg\"\n".into(),
            ),
            "/w/inc/routes.cfg" => Some("route[R] { exit; }\n".into()),
            _ => None,
        }
    };
    let root_text = loader(Path::new("/w/kamailio.cfg")).unwrap();
    let files = include_closure(Path::new("/w/kamailio.cfg"), &root_text, &loader);
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.to_str().unwrap()).collect();
    assert_eq!(paths.len(), 2, "one root and one include: {paths:?}");
}

/// Randomised include graphs: the invariants, not the answers.
///
/// A hand-written fixture proves the cases its author thought of, and
/// every defect found in this feature so far lived in a case nobody
/// had thought of.  These build thousands of graphs from a fixed seed
/// — chains, diamonds, cycles, self-includes, orphans, files reached
/// by two spellings — and assert what has to hold for every one.
/// Fixed seed, so a failure is reproducible rather than a rumour.
#[test]
fn include_graph_invariants_hold_over_random_graphs() {
    use kamailio_lsp::logic::IncludeGraph;
    use std::path::{Path, PathBuf};

    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut rng = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    for case in 0..3000u32 {
        let n = 2 + (rng() % 7) as usize;
        let names: Vec<String> = (0..n).map(|i| format!("/w/c{i}.cfg")).collect();
        let mut texts = vec![String::new(); n];
        for text in texts.iter_mut() {
            // each config includes a random subset of the others,
            // written in a random spelling of the same path
            for j in 0..n {
                if rng() % 3 != 0 {
                    continue;
                }
                let spelling = match rng() % 4 {
                    0 => format!("c{j}.cfg"),
                    1 => format!("./c{j}.cfg"),
                    2 => format!("../w/c{j}.cfg"),
                    _ => format!("/w/c{j}.cfg"),
                };
                text.push_str(&format!("include_file \"{spelling}\"\n"));
            }
        }
        let configs: Vec<(PathBuf, String)> = names
            .iter()
            .map(PathBuf::from)
            .zip(texts.iter().cloned())
            .collect();
        let g = IncludeGraph::build(&configs);
        // scan order must not decide anything: a fragment's context
        // cannot depend on which file the directory walk saw first
        let mut shuffled = configs.clone();
        shuffled.reverse();
        let g2 = IncludeGraph::build(&shuffled);

        for name in &names {
            let p = Path::new(name);
            // terminates (a hang fails the suite by timeout), and
            // never claims a file is its own root
            let root = g.analysis_root(p);
            assert_ne!(
                root.as_deref(),
                Some(p),
                "case {case}: {name} is its own root"
            );
            assert_eq!(
                root,
                g2.analysis_root(p),
                "case {case}: scan order changed the answer for {name}"
            );
            // whatever comes back must be reachable by walking
            // parents from the fragment — not merely plausible
            if let Some(r) = root {
                let mut cur = p.to_path_buf();
                let mut seen = std::collections::HashSet::new();
                let mut reached = false;
                while seen.insert(cur.clone()) {
                    match g.analysis_root(&cur) {
                        Some(up) => {
                            if up == r {
                                reached = true;
                                break;
                            }
                            cur = up;
                        }
                        None => break,
                    }
                }
                assert!(reached, "case {case}: {r:?} is not an ancestor of {name}");
            }
        }
    }
}

/// Every spelling of one path resolves to one key, and folding is
/// idempotent — otherwise "the same file" depends on how it was
/// written, which is how a fragment ends up with no root and a
/// closure that visits it twice.
#[test]
fn resolved_includes_fold_every_spelling_to_one_key() {
    use kamailio_lsp::logic::resolved_includes;
    use std::path::{Component, Path};
    let from = Path::new("/w/sites/site.cfg");
    let spellings = [
        "../common/r.cfg",
        ".././common/r.cfg",
        "../common/./r.cfg",
        "../common/x/../r.cfg",
        "/w/common/r.cfg",
        "./../common/r.cfg",
    ];
    let text: String = spellings
        .iter()
        .map(|s| format!("include_file \"{s}\"\n"))
        .collect();
    let got = resolved_includes(from, &text);
    assert_eq!(got.len(), spellings.len(), "one entry per directive");
    for (spelling, path) in spellings.iter().zip(&got) {
        assert_eq!(
            path,
            Path::new("/w/common/r.cfg"),
            "{spelling} names the same file as the others"
        );
        // nothing foldable is left behind
        assert!(
            !path
                .components()
                .any(|c| matches!(c, Component::CurDir | Component::ParentDir)),
            "{path:?} still carries a . or .. component"
        );
    }
    // folding what is already folded changes nothing
    let again = resolved_includes(from, "include_file \"/w/common/r.cfg\"\n");
    assert_eq!(again, vec![std::path::PathBuf::from("/w/common/r.cfg")]);
}

/// Model-based: random operation sequences against an independent
/// model of the same question.
///
/// The graph is rebuilt from four places and read from many more.  A
/// stale answer is not an error — it is a correct-looking answer for
/// a workspace that no longer exists, which no single-step test can
/// see.  Here a plain model computes the root by climbing
/// first-parents, a sequence of random edits is applied to both, and
/// every file is compared after every step.
#[test]
fn the_graph_tracks_a_model_through_random_edits() {
    use kamailio_lsp::logic::IncludeGraph;
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use std::path::{Path, PathBuf};

    /// The answer, computed the obvious slow way.
    fn model_root(inc: &BTreeMap<String, BTreeSet<String>>, f: &str) -> Option<String> {
        let mut parents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (src, tgts) in inc {
            for t in tgts {
                parents.entry(t.as_str()).or_default().push(src.as_str());
            }
        }
        for v in parents.values_mut() {
            v.sort();
        }
        let mut seen: HashSet<&str> = HashSet::new();
        seen.insert(f);
        let mut best: Option<String> = None;
        let mut cur = f;
        loop {
            let Some(next) = parents.get(cur).and_then(|v| v.first()).copied() else {
                return best;
            };
            if !seen.insert(next) {
                return best;
            }
            best = Some(next.to_string());
            cur = next;
        }
    }

    let names: Vec<String> = (0..6).map(|i| format!("/w/c{i}.cfg")).collect();
    let mut inc: BTreeMap<String, BTreeSet<String>> =
        names.iter().map(|n| (n.clone(), BTreeSet::new())).collect();

    let mut state = 0xDEAD_BEEF_CAFE_F00Du64;
    let mut rng = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    for step in 0..600u32 {
        let f = names[(rng() % names.len() as u64) as usize].clone();
        let t = names[(rng() % names.len() as u64) as usize].clone();
        if rng() % 2 == 0 {
            if f != t {
                inc.get_mut(&f).unwrap().insert(t);
            }
        } else {
            let victim = inc[&f].iter().next().cloned();
            if let Some(v) = victim {
                inc.get_mut(&f).unwrap().remove(&v);
            }
        }
        let configs: Vec<(PathBuf, String)> = names
            .iter()
            .map(|n| {
                let body: String = inc[n]
                    .iter()
                    // every spelling of the same target, in turn
                    .enumerate()
                    .map(|(k, t)| match k % 3 {
                        0 => format!("include_file \"{t}\"\n"),
                        1 => format!("include_file \"{}\"\n", t.replace("/w/", "./")),
                        _ => format!("include_file \"{}\"\n", t.replace("/w/", "../w/")),
                    })
                    .collect();
                (PathBuf::from(n), body)
            })
            .collect();
        let g = IncludeGraph::build(&configs);
        for n in &names {
            assert_eq!(
                g.analysis_root(Path::new(n))
                    .map(|p| p.display().to_string()),
                model_root(&inc, n),
                "step {step}: {n} disagrees with the model; includes = {inc:?}"
            );
        }
    }
}

/// The workspace sweep must reach a configuration that is not named
/// `*.cfg`.
///
/// A tree whose root is `proxy.inc` — or an `.m4` template — was
/// invisible to the sweep, so no fragment under it ever resolved a
/// root and every one of them was analysed alone.  The sweep still
/// must not read the whole tree: it looks for configurations, not for
/// every file in the folder.
#[test]
fn the_workspace_sweep_reaches_configs_not_named_cfg() {
    use kamailio_lsp::logic::{configs_in_dir, scan_configs};

    let dir = std::env::temp_dir().join(format!("kamlsp-sweep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("include")).unwrap();
    std::fs::write(
        dir.join("proxy.inc"),
        "include_file \"include/routes.inc\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("kamailio.m4"),
        "include_file \"include/routes.inc\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("include/routes.inc"), "request_route { exit; }\n").unwrap();
    std::fs::write(dir.join("notes.md"), "not a config\n").unwrap();

    let (found, _) = scan_configs(std::slice::from_ref(&dir), 500);
    let names: Vec<String> = found
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    for want in ["proxy.inc", "kamailio.m4", "routes.inc"] {
        assert!(
            names.contains(&want.to_string()),
            "{want} missing: {names:?}"
        );
    }
    assert!(
        !names.contains(&"notes.md".to_string()),
        "the sweep must not collect every file: {names:?}"
    );

    let here: Vec<String> = configs_in_dir(&dir)
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    assert!(here.contains(&"proxy.inc".to_string()), "{here:?}");
    assert!(!here.contains(&"notes.md".to_string()), "{here:?}");

    std::fs::remove_dir_all(&dir).unwrap();
}

/// A module the harvester read nothing from documents nothing, and an
/// empty list is not evidence that a parameter does not exist.
///
/// The sibling found this on `auth_web3`, whose README uses a shape
/// the harvester does not read: once such a module is in the
/// catalogue at all, every `modparam` for it reads as undocumented.
/// A module with functions but no parameters WAS read, so `textops`
/// keeps its true positives.
#[test]
fn a_module_documenting_nothing_at_all_stays_silent() {
    use kamailio_lsp::catalog::{CatalogOrigin, Item, ModuleDoc};
    use kamailio_lsp::logic::catalog_diagnostics;

    let origin = CatalogOrigin::BuiltIn("6.1.4".to_string());
    let nothing = vec![ModuleDoc {
        name: "unharvested".into(),
        params: Vec::new(),
        functions: Vec::new(),
    }];
    assert!(
        catalog_diagnostics(&nothing, &origin, "modparam(\"unharvested\", \"p\", 1)\n").is_empty(),
        "an unharvested module must not accuse the config"
    );

    let functions_only = vec![ModuleDoc {
        name: "textops".into(),
        params: Vec::new(),
        functions: vec![Item {
            name: "search".into(),
            detail: String::new(),
            doc: String::new(),
        }],
    }];
    assert_eq!(
        catalog_diagnostics(
            &functions_only,
            &origin,
            "modparam(\"textops\", \"nope\", 1)\n"
        )
        .len(),
        1,
        "a module with functions but no parameters was read, and exports none"
    );
}
