use kamailio_lsp::analyze::{loaded_modules, modparam_context, route_defs, route_refs, word_at};

const CFG: &str = r#"#!KAMAILIO
# reachability-only config
#!define WITH_OPTIONS
#!ifdef WITH_OPTIONS
loadmodule "sl.so"
#!endif
loadmodule "tm"
#loadmodule "commented_out.so"
/* loadmodule "block_commented.so" */
loadmodule "htable.so"

modparam("htable", "htable", "a=>size=4;")
modparam("htable", "db_url", "mysql://user:pass@192.0.2.7/kamailio")

request_route {
    if (is_method("OPTIONS")) {
        sl_send_reply("200", "OK");   # has a # inside a line with strings
        exit;
    }
    route(CHECK_SOURCE);
    route("QUOTED");
    xlog("hash # inside string is not a comment: loadmodule \"fake.so\"");
}

route[CHECK_SOURCE]
{
    if (!ds_is_from_list()) {
        sl_send_reply("403", "Forbidden");
    }
}

route["QUOTED"] {
    exit;
}

failure_route[NAT_FAIL] {
    t_reply("500", "err");
}

onreply_route[REPLY_ONE] { exit; }
branch_route[BR_ONE] { exit; }
onsend_route {
    drop;
}
reply_route {
    exit;
}
event_route[htable:mod-init] {
    exit;
}
"#;

#[test]
fn finds_loaded_modules_skipping_comments_and_bare_names() {
    let mods: Vec<String> = loaded_modules(CFG).into_iter().map(|m| m.name).collect();
    // bare `loadmodule "tm"` (no .so) is valid Kamailio and must count
    assert_eq!(mods, vec!["sl", "tm", "htable"]);
}

#[test]
fn loadmodule_inside_string_is_not_collected() {
    let mods = loaded_modules(CFG);
    assert!(!mods.iter().any(|m| m.name == "fake"));
}

#[test]
fn preprocessor_lines_do_not_swallow_the_next_line() {
    // #!ifdef is a directive, not a comment that eats the loadmodule
    let mods = loaded_modules(CFG);
    assert!(mods.iter().any(|m| m.name == "sl"));
}

#[test]
fn finds_route_definitions_of_every_kamailio_kind() {
    let defs = route_defs(CFG);
    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"CHECK_SOURCE"));
    assert!(names.contains(&"QUOTED")); // quoted name, unquoted in the result
    assert!(names.contains(&"NAT_FAIL"));
    assert!(names.contains(&"REPLY_ONE"));
    assert!(names.contains(&"BR_ONE"));
    assert!(names.contains(&"htable:mod-init")); // event_route[mod:event]
    // request_route / onsend_route / reply_route define unnamed blocks
    assert_eq!(
        defs.iter().filter(|d| d.name.is_empty()).count(),
        3,
        "request_route, onsend_route, reply_route: {names:?}"
    );
    let cs = defs.iter().find(|d| d.name == "CHECK_SOURCE").unwrap();
    assert_eq!(cs.line, 24);
}

#[test]
fn opensips_only_route_kinds_are_not_kamailio_defs() {
    for cfg in [
        "timer_route[x, 30] { exit; }\n",
        "error_route { exit; }\n",
        "local_route { exit; }\n",
        "startup_route { exit; }\n",
    ] {
        let defs = route_defs(cfg);
        assert!(
            defs.is_empty(),
            "opensips-only kind must not define a route: {cfg:?} -> {defs:?}"
        );
    }
}

#[test]
fn finds_route_references_quoted_and_bare() {
    let refs = route_refs(CFG);
    let names: Vec<&str> = refs.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["CHECK_SOURCE", "QUOTED"]);
    assert_eq!(refs[0].line, 19);
}

#[test]
fn modparam_context_detects_module_of_param_position() {
    let line = r#"modparam("htable", "db_"#;
    assert_eq!(modparam_context(line), Some("htable".to_string()));
    assert_eq!(
        modparam_context(r#"modparam("tm", "#),
        Some("tm".to_string())
    );
    assert_eq!(modparam_context("xlog(\"nope\""), None);
    // first argument still open → no module context yet
    assert_eq!(modparam_context(r#"modparam("hta"#), None);
}

#[test]
fn word_at_extracts_identifier() {
    let text = "    sl_send_reply(\"200\", \"OK\");";
    assert_eq!(word_at(text, 6), Some("sl_send_reply".to_string()));
    assert_eq!(word_at(text, 0), None);
    assert_eq!(word_at("", 5), None);
    // out-of-range column must not panic
    assert_eq!(word_at("ab", 99), None);
}

#[test]
fn adversarial_inputs_do_not_panic() {
    for s in [
        "",
        "\0\0\0",
        "loadmodule \"\0.so\"",
        "route[",
        "#",
        "#!",
        "#!define",
        "\\",
        "modparam(\"",
        "route[\\\"x]",
        "event_route[:",
        "route[\"unterminated]",
    ] {
        let _ = loaded_modules(s);
        let _ = route_defs(s);
        let _ = route_refs(s);
        let _ = modparam_context(s);
        let _ = word_at(s, 3);
    }
}

use kamailio_lsp::analyze::route_blocks;

#[test]
fn route_blocks_report_full_extents() {
    let text = "loadmodule \"tm.so\"\nrequest_route {\n    if (1) {\n        exit;\n    }\n}\nfailure_route[FR] {\n    xlog(\"x\");\n}\n";
    let blocks = route_blocks(text);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].kind, "request_route");
    assert_eq!(blocks[0].name, "");
    assert_eq!((blocks[0].line, blocks[0].col), (1, 0));
    // nested braces are matched: the block ends at line 5's `}`
    assert_eq!((blocks[0].end_line, blocks[0].end_col), (5, 1));
    assert_eq!(blocks[1].kind, "failure_route");
    assert_eq!(blocks[1].name, "FR");
    assert_eq!((blocks[1].line, blocks[1].end_line), (6, 8));
}

#[test]
fn route_blocks_ignore_braces_in_strings_and_comments() {
    let text = "request_route {\n    xlog(\"}\");\n    # }\n    /* } */\n    exit;\n}\n";
    let blocks = route_blocks(text);
    assert_eq!(blocks.len(), 1);
    assert_eq!((blocks[0].end_line, blocks[0].end_col), (5, 1));
}

#[test]
fn event_route_block_extents_carry_the_colon_name() {
    let text = "event_route[htable:mod-init] {\n    exit;\n}\n";
    let blocks = route_blocks(text);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].kind, "event_route");
    assert_eq!(blocks[0].name, "htable:mod-init");
    assert_eq!(blocks[0].end_line, 2);
}

#[test]
fn unterminated_route_block_extends_to_eof() {
    let text = "request_route {\n    exit;\n";
    let blocks = route_blocks(text);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].end_line, 2);
}

#[test]
fn route_blocks_adversarial_do_not_panic() {
    for s in [
        "",
        "route {",
        "route }{",
        "route {{{{",
        "route[\u{0}] {}",
        "route[a] { \"unterminated",
        "}}}}",
        "request_route {\\",
    ] {
        let _ = route_blocks(s);
    }
}

use kamailio_lsp::analyze::includes;

#[test]
fn includes_are_extracted_from_code_position_only() {
    let text = "include_file \"a.cfg\"\nimport_file \"sub/b.cfg\"\n# include_file \"no.cfg\"\n/* include_file \"no2.cfg\" */\nxlog(\"include_file \\\"no3.cfg\\\"\");\n";
    let inc = includes(text);
    let names: Vec<&str> = inc.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["a.cfg", "sub/b.cfg"]);
    assert_eq!(inc[0].line, 0);
    assert_eq!(inc[1].line, 1);
}

#[test]
fn includes_adversarial_do_not_panic() {
    for s in [
        "",
        "include_file",
        "include_file \"",
        "include_file \"\0\"",
        "import_file \"\\\"\"",
        "reinclude_file \"x\"",
    ] {
        let _ = includes(s);
    }
    // an identifier tail must not match (reinclude_file)
    assert!(includes("reinclude_file \"x.cfg\"").is_empty());
}

#[test]
fn prefixed_include_directives_are_found() {
    // cfg.lex: PREP_START = "#!" | "!!", and include_file/import_file
    // exist bare AND prefixed; directives are not line-anchored
    let text = "#!include_file \"a.cfg\"\n  !!import_file \"b.cfg\"\ninclude_file \"c.cfg\"\n";
    let names: Vec<String> = includes(text).into_iter().map(|i| i.name).collect();
    assert_eq!(names, vec!["a.cfg", "b.cfg", "c.cfg"]);
}

#[test]
fn define_bodies_are_directive_text_not_code() {
    // a multi-line #!define with backslash continuations: nothing in
    // the body may leak phantom modules/routes/refs into the analysis
    let text = "#!define LOOP loadmodule \"fake.so\" \\\n    route[PHANTOM] { \\\n    route(GHOST); }\nloadmodule \"tm.so\"\nrequest_route { exit; }\n";
    assert_eq!(
        loaded_modules(text)
            .into_iter()
            .map(|m| m.name)
            .collect::<Vec<_>>(),
        vec!["tm"],
        "define bodies must not produce loadmodules"
    );
    assert!(
        route_defs(text).iter().all(|d| d.name != "PHANTOM"),
        "define bodies must not produce route defs"
    );
    assert!(
        route_refs(text).is_empty(),
        "define bodies must not produce route refs"
    );
}

#[test]
fn double_bang_directives_are_not_code() {
    let text = "!!define X loadmodule \"no.so\"\nloadmodule \"tm.so\"\n";
    let mods: Vec<String> = loaded_modules(text).into_iter().map(|m| m.name).collect();
    assert_eq!(mods, vec!["tm"]);
}

#[test]
fn ifdef_branches_are_both_scanned() {
    // deliberate: the analyzer sees both sides of #!ifdef/#!else
    let text = "#!ifdef A\nloadmodule \"x.so\"\n#!else\nloadmodule \"y.so\"\n#!endif\n";
    let mods: Vec<String> = loaded_modules(text).into_iter().map(|m| m.name).collect();
    assert_eq!(mods, vec!["x", "y"]);
}

#[test]
fn slash_slash_line_comments_are_comments() {
    // cfg.lex: COM_LINE = "#" | "//"
    let text = "// loadmodule \"no.so\"\nloadmodule \"tm.so\"\n";
    let mods: Vec<String> = loaded_modules(text).into_iter().map(|m| m.name).collect();
    assert_eq!(mods, vec!["tm"]);
}

#[test]
fn preproc_adversarial_do_not_panic() {
    for s in [
        "#!",
        "!!",
        "#!definex \"x\"",
        "#!define X \\",
        "#!define X \\\n",
        "#!define \\\n\\\n\\",
        "  #!ifdef Y",
        "!!\\",
        "#!define A route[",
    ] {
        let _ = loaded_modules(s);
        let _ = route_defs(s);
        let _ = route_refs(s);
        let _ = includes(s);
    }
    // a define body reaching EOF via continuation must not panic and
    // must not leak code
    assert!(route_defs("#!define X route[EOF] { \\").is_empty());
}

#[test]
fn paren_and_x_load_forms_are_collected() {
    // cfg.y: LOADMODULE LPAREN STRING [COMMA STRING] RPAREN and
    // loadmodulex are all legal (verified rc=0 on 6.0.1 and 6.1.4)
    let text = "loadmodule(\"tm.so\")\nloadmodule(\"htable.so\", \"opts\")\nloadmodulex \"pv.so\"\nloadmodulex(\"sl.so\")\nmyloadmodule \"no.so\"\n";
    let mods: Vec<String> = loaded_modules(text).into_iter().map(|m| m.name).collect();
    assert_eq!(mods, vec!["tm", "htable", "pv", "sl"]);
}

#[test]
fn modparamx_context_is_recognized() {
    assert_eq!(
        modparam_context(r#"modparamx("htable", ""#),
        Some("htable".to_string())
    );
}

#[test]
fn single_quoted_strings_are_strings() {
    // cfg.lex STRING2: tick-delimited strings are legal anywhere
    // (verified rc=0: loadmodule 'tm.so', route['SQ'], 'a' == 'a')
    let text = "loadmodule 'tm.so'\nrequest_route {\n    xlog('loadmodule \"fake.so\"');\n    route('SQ');\n}\nroute['SQ'] {\n    exit;\n}\n";
    let mods: Vec<String> = loaded_modules(text).into_iter().map(|m| m.name).collect();
    // the single-quoted loadmodule argument IS collected; the one
    // inside the single-quoted xlog string is NOT
    assert_eq!(mods, vec!["tm"], "{mods:?}");
    let refs: Vec<String> = route_refs(text).into_iter().map(|r| r.name).collect();
    assert_eq!(refs, vec!["SQ"]);
    let defs: Vec<String> = route_defs(text)
        .into_iter()
        .map(|d| d.name)
        .filter(|n| !n.is_empty())
        .collect();
    assert_eq!(defs, vec!["SQ"]);
}

#[test]
fn single_quoted_strings_span_lines_without_escapes() {
    // STRING2 consumes CRs and has no escape processing: a backslash
    // does not extend the string past the closing tick
    let text = "request_route {\n    xlog('line one\nroute[IN_STR] { exit; }\n');\n    xlog('ends here\\');\n    route(AFTER);\n}\n";
    assert!(
        route_defs(text).iter().all(|d| d.name != "IN_STR"),
        "multi-line single-quoted interior is a string"
    );
    let refs: Vec<String> = route_refs(text).into_iter().map(|r| r.name).collect();
    assert_eq!(refs, vec!["AFTER"], "backslash does not escape the tick");
}

#[test]
fn single_quote_adversarial_do_not_panic() {
    for s in [
        "'",
        "''",
        "'unterminated",
        "route[']",
        "'\0'",
        "loadmodule '",
    ] {
        let _ = loaded_modules(s);
        let _ = route_defs(s);
        let _ = route_refs(s);
        let _ = includes(s);
    }
}

use kamailio_lsp::analyze::modparam_calls;

#[test]
fn modparam_calls_are_extracted_with_param_positions() {
    let text = "modparam(\"tm\", \"fr_timer\", 5)\n# modparam(\"no\", \"x\", 1)\nmodparam('sq', 'p2', \"v\")\nmodparamx(\"htable\", \"htable\", \"a=>size=4;\")\n";
    let mp = modparam_calls(text);
    assert_eq!(mp.len(), 3, "{mp:?}");
    assert_eq!(
        (mp[0].module.as_str(), mp[0].param.as_str()),
        ("tm", "fr_timer")
    );
    assert_eq!(mp[0].line, 0);
    // col points at the PARAM name (inside its quotes)
    assert_eq!(mp[0].col, 16);
    assert_eq!((mp[1].module.as_str(), mp[1].param.as_str()), ("sq", "p2"));
    // modparamx counts too (cfg.y MODPARAMX)
    assert_eq!(mp[2].module.as_str(), "htable");
    // adversarial
    for s in [
        "",
        "modparam(",
        "modparam(\"a\"",
        "modparam(\"a\",\"\0\",1)",
        "xmodparam(\"a\",\"b\",1)",
    ] {
        let _ = modparam_calls(s);
    }
    assert!(
        modparam_calls("xmodparam(\"a\",\"b\",1)").is_empty(),
        "identifier tails must not match"
    );
}

#[test]
fn include_links_carry_the_path_span() {
    use kamailio_lsp::analyze::include_links;
    let text = "include_file \"inc/a.cfg\"\n#!import_file \"b.cfg\"\nrequest_route { exit; }\n";
    let links = include_links(text);
    assert_eq!(links.len(), 2, "{links:?}");
    // spans cover the QUOTED PATH, not the directive keyword
    assert_eq!(links[0].path, "inc/a.cfg");
    assert_eq!(links[0].line, 0);
    assert_eq!(links[0].col, 14, "starts inside the quotes");
    assert_eq!(links[0].len, "inc/a.cfg".len() as u32);
    assert_eq!(links[1].path, "b.cfg");
    assert_eq!(links[1].line, 1);
    assert_eq!(links[1].col, 15);
    assert_eq!(links[1].len, 5);
}

#[test]
fn include_links_skip_comments_strings_and_hostile_paths() {
    use kamailio_lsp::analyze::include_links;
    // commented, string-embedded, and NUL-carrying paths yield nothing
    assert!(include_links("# include_file \"a.cfg\"\n").is_empty());
    assert!(include_links("$var(x) = \"include_file \\\"a.cfg\\\"\";\n").is_empty());
    assert!(include_links("include_file \"a\u{0}b.cfg\"\n").is_empty());
    assert!(include_links("include_file \"\"\n").is_empty());
    // adversarial soup must not panic
    for s in [
        "include_file \"\\\\\\\\weird\\\\path.cfg\"",
        "include_file 'single.cfg'",
        "!!include_file \"bang.cfg\"",
        "include_file \"unterminated",
        "\u{0}\u{0}include_file \"x\"",
    ] {
        let _ = include_links(s);
    }
    // single quotes and !! prefixes DO produce links
    assert_eq!(
        include_links("include_file 'single.cfg'\n")[0].path,
        "single.cfg"
    );
    assert_eq!(
        include_links("!!include_file \"bang.cfg\"\n")[0].path,
        "bang.cfg"
    );
}

/// The preprocessor decides which includes exist.
///
/// A file inside a `#!ifdef` for a symbol nobody defined is never
/// opened by the real parser — it does not even report syntax errors
/// in it — so treating it as part of that configuration is worse than
/// treating it as part of nothing: it gets analysed against a program
/// it is not in, and the checker runs on a root that never reads it.
#[test]
fn active_includes_skips_what_the_preprocessor_compiles_out() {
    use kamailio_lsp::analyze::active_includes;
    let names =
        |t: &str| -> Vec<String> { active_includes(t).into_iter().map(|i| i.name).collect() };

    // plain, unconditional
    assert_eq!(names("include_file \"a.cfg\"\n"), vec!["a.cfg"]);

    // guarded by a symbol defined above it: the parser reads it
    assert_eq!(
        names("#!define ON\n#!ifdef ON\ninclude_file \"a.cfg\"\n#!endif\n"),
        vec!["a.cfg"]
    );

    // guarded by a symbol nobody defines: the parser never opens it
    assert!(names("#!ifdef OFF\ninclude_file \"a.cfg\"\n#!endif\n").is_empty());

    // the define must come FIRST, as the preprocessor reads top-down
    assert!(names("#!ifdef ON\ninclude_file \"a.cfg\"\n#!endif\n#!define ON\n").is_empty());

    // `#!else` of a satisfied ifdef is dead
    assert_eq!(
        names(
            "#!define ON\n#!ifdef ON\ninclude_file \"y.cfg\"\n#!else\ninclude_file \"n.cfg\"\n#!endif\n"
        ),
        vec!["y.cfg"]
    );

    // nesting: an inner conditional inside a dead outer one stays dead
    assert!(
        names("#!ifdef OFF\n#!define IN\n#!ifdef IN\ninclude_file \"a.cfg\"\n#!endif\n#!endif\n")
            .is_empty()
    );

    // and everything after `#!endif` is live again
    assert_eq!(
        names("#!ifdef OFF\ninclude_file \"a.cfg\"\n#!endif\ninclude_file \"b.cfg\"\n"),
        vec!["b.cfg"]
    );

    // `#!ifndef` is never claimed: the symbol may be defined by
    // something this file cannot see
    assert!(names("#!ifndef OFF\ninclude_file \"a.cfg\"\n#!endif\n").is_empty());

    // the lenient view still sees them all — the closure must not
    // lose routes just because a conditional is unresolved
    assert_eq!(
        kamailio_lsp::analyze::includes("#!ifdef OFF\ninclude_file \"a.cfg\"\n#!endif\n").len(),
        1
    );

    // adversarial: never panic
    for t in [
        "#!endif\n",
        "#!ifdef\n",
        "#!else\n#!endif\n#!endif\n",
        "#!ifdef A\n",
        "!!ifdef A\ninclude_file \"a.cfg\"\n!!endif\n",
        "\0#!ifdef A\n",
    ] {
        let _ = active_includes(t);
    }
}
