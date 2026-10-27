//! Whole-tree harvesting: `harvest_tree` walks the module READMEs of
//! a Kamailio source tree (`src/modules/<name>/README`).

mod common;

use kamailio_lsp::catalog::harvest_tree;

fn write(path: &std::path::Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

const MINI_README: &str = r#"Mini Module

2. Parameters

2.1. my_param (int)

   A parameter.

3. Functions

3.1.  my_func(arg)

   Does things.
"#;

#[test]
fn harvests_the_kamailio_src_modules_layout() {
    let scratch = common::Scratch::new("kamlsp-harv");
    let root = scratch.path().to_path_buf();
    write(&root.join("src/modules/mini/README"), MINI_README);
    // a module without exported chapters still harvests (empty)
    write(
        &root.join("src/modules/bare/README"),
        "Bare Module\n\n   Prose.\n",
    );
    // a stray file (not a directory) must be skipped, not crash
    write(&root.join("src/modules/CMakeLists.txt"), "x\n");
    let mods = harvest_tree(&root);
    let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["bare", "mini"]);
    let mini = &mods[1];
    assert_eq!(mini.params[0].name, "my_param");
    assert_eq!(mini.functions[0].name, "my_func");
}

#[test]
fn falls_back_to_a_bare_modules_dir() {
    // tolerate being pointed at src/ itself, or at an installed
    // doc tree that has modules/ at the top
    let scratch = common::Scratch::new("kamlsp-harv-flat");
    let root = scratch.path().to_path_buf();
    write(&root.join("modules/mini/README"), MINI_README);
    let mods = harvest_tree(&root);
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].name, "mini");
}

#[test]
fn missing_tree_harvests_empty() {
    let mods = harvest_tree(std::path::Path::new("/nonexistent/kamailio"));
    assert!(mods.is_empty());
}

#[test]
fn harvests_the_real_kamailio_tree_when_present() {
    // point KAMAILIO_LSP_TEST_TREE at a Kamailio source tree to run
    let tree = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let root = std::path::Path::new(&tree);
    let mods = harvest_tree(root);
    assert!(
        mods.len() > 200,
        "expected >200 documented modules, got {}",
        mods.len()
    );
    // tm exists in every Kamailio tree
    let tm = mods.iter().find(|m| m.name == "tm").expect("tm doc");
    assert!(tm.params.iter().any(|p| p.name == "fr_timer"));
    assert!(tm.functions.iter().any(|f| f.name == "t_relay"));
    assert!(tm.params.len() > 40, "tm params: {}", tm.params.len());
    assert!(
        tm.functions.len() > 40,
        "tm functions: {}",
        tm.functions.len()
    );
    // spot-check more everyday modules
    let sl = mods.iter().find(|m| m.name == "sl").expect("sl doc");
    assert!(sl.functions.iter().any(|f| f.name == "sl_send_reply"));
    let ht = mods
        .iter()
        .find(|m| m.name == "htable")
        .expect("htable doc");
    assert!(ht.params.iter().any(|p| p.name == "htable"));
    assert!(ht.functions.iter().any(|f| f.name == "sht_lock"));
    let disp = mods
        .iter()
        .find(|m| m.name == "dispatcher")
        .expect("dispatcher");
    assert!(disp.functions.iter().any(|f| f.name == "ds_select_dst"));
    // modules whose READMEs use lowercase "Exported parameters" ...
    let nat = mods
        .iter()
        .find(|m| m.name == "nat_traversal")
        .expect("nat_traversal");
    assert!(
        nat.params.iter().any(|p| p.name == "keepalive_interval"),
        "nat_traversal params: {:?}",
        nat.params.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    assert!(nat.functions.iter().any(|f| f.name == "nat_keepalive"));
    let cc = mods
        .iter()
        .find(|m| m.name == "call_control")
        .expect("call_control");
    assert!(!cc.params.is_empty(), "call_control params empty");
    // ... and modules with nested chapters (2.3. Parameters)
    let kafka = mods.iter().find(|m| m.name == "kafka").expect("kafka");
    assert!(
        kafka.functions.iter().any(|f| f.name == "kafka_send"),
        "kafka functions: {:?}",
        kafka.functions.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
    assert!(kafka.params.iter().any(|p| p.name == "brokers"));
    for m in ["keepalive", "lrkproxy", "seas"] {
        let md = mods.iter().find(|x| x.name == m).expect(m);
        assert!(
            !md.params.is_empty() || !md.functions.is_empty(),
            "{m} harvested empty"
        );
    }
    // sanity: no harvested doc may carry raw HTML into hover popups
    for m in &mods {
        for i in m.params.iter().chain(m.functions.iter()) {
            assert!(
                !i.doc.contains('<') || !i.doc.contains('>') || !i.doc.contains("</"),
                "{}::{} doc looks like raw html: {}",
                m.name,
                i.name,
                i.doc
            );
        }
    }
}

#[test]
fn the_shipped_admin_doc_documents_every_init_option() {
    // docs/ADMIN.md is the option reference; every initialization
    // option the server understands must appear as a `#### name`
    // heading with an environment/default note.
    let md = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/ADMIN.md"))
        .expect("docs/ADMIN.md must exist");
    // The list is DERIVED from the server, not written out here.  A
    // hand-maintained list cannot fail for the one thing it exists to
    // catch: this check carried a literal array written before inlay
    // hints existed, so new settings went undocumented while it
    // reported green.
    //
    // Whitespace is stripped before scanning because rustfmt wraps the
    // call as `opts\n.get("name")`, and an extraction looking for the
    // literal `opts.get("` silently finds nothing there — a gate
    // defeated by the formatter CI itself enforces.
    let server = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/server.rs"))
        .expect("src/server.rs");
    let flat: String = server.chars().filter(|c| !c.is_whitespace()).collect();
    let mut opts: Vec<String> = Vec::new();
    for (i, _) in flat.match_indices("opts.get(\"") {
        if let Some(name) = flat[i + 10..].split('"').next()
            && name != "kamailioLsp"
            && !opts.iter().any(|o| o == name)
        {
            opts.push(name.to_string());
        }
    }
    assert!(
        opts.len() >= 8,
        "only {} init options found — the scan went blind: {opts:?}",
        opts.len()
    );
    for opt in &opts {
        assert!(
            md.contains(&format!("#### {opt}")),
            "{opt} is read by the server but docs/ADMIN.md has no option section for it"
        );
    }
    for env in [
        "KAMAILIO_LSP_BIN",
        "KAMAILIO_LSP_SRC",
        "KAMAILIO_LSP_WIKI",
        "KAMAILIO_LSP_CACHE_DIR",
        "KAMAILIO_LSP_CHECK_TIMEOUT_MS",
    ] {
        assert!(md.contains(env), "docs/ADMIN.md is missing env var {env}");
    }
}

/// A `cmd_export_t` entry whose handler and parameter count are a
/// documented function's, under a name the docs never use, is that
/// function under a second name. `sl.c` (6.1.5, lines 103-104) exports
/// `sl_send_reply` and `sl_reply` both as `w_sl_send_reply` with 2
/// parameters, and the README documents only `sl_send_reply`, so
/// `sl_reply` had no completion, hover or signature help.
#[test]
fn an_undocumented_export_sharing_a_documented_handler_is_an_alias() {
    use kamailio_lsp::catalog::{Item, function_aliases_from_c};
    let c = r#"
static cmd_export_t cmds[] = {
	{"sl_send_reply", w_sl_send_reply, 2, fixup_sl_reply, fixup_free_fparam_all, REQUEST_ROUTE},
	{"sl_reply", w_sl_send_reply, 2, fixup_sl_reply, fixup_free_fparam_all, REQUEST_ROUTE},
	{"sl_reply_error", (cmd_function)w_sl_reply_error, 0, 0, 0, REQUEST_ROUTE},
	/* {"old_name", w_sl_reply_error, 0, 0, 0, REQUEST_ROUTE}, */
	// {"older_name", w_sl_reply_error, 0, 0, 0, REQUEST_ROUTE},
	{"t_relay", w_t_relay, 0, 0, 0, REQUEST_ROUTE},
	{"t_relay_two", w_t_relay, 2, 0, 0, REQUEST_ROUTE},
	{"undocumented", other_f, 0, 0, 0, REQUEST_ROUTE},
	{0, 0, 0, 0, 0, 0}
};
static param_export_t params[] = {
	{"default_code", PARAM_INT, &default_code},
	{"bind_code", PARAM_INT, &bind_code},
	{0, 0, 0}
};
"#;
    let documented = vec![
        Item {
            name: "sl_send_reply".into(),
            detail: "sl_send_reply(code, reason)".into(),
            doc: "Send a stateless reply.".into(),
        },
        Item {
            name: "sl_reply_error".into(),
            detail: "sl_reply_error()".into(),
            doc: "Reply with the last internal error.".into(),
        },
        Item {
            name: "t_relay".into(),
            detail: "t_relay()".into(),
            doc: "Relay statefully.".into(),
        },
    ];
    let got: Vec<(String, String, String)> = function_aliases_from_c(c, &documented)
        .into_iter()
        .map(|i| (i.name, i.detail, i.doc))
        .collect();
    // the negative controls: a commented-out entry is not exported, the
    // same handler with another parameter count is not the same
    // function, an export with a handler of its own is nobody's alias,
    // and a parameter table is not a function table
    assert_eq!(
        got,
        vec![(
            "sl_reply".to_string(),
            "sl_reply(code, reason)".to_string(),
            "Alias of `sl_send_reply`: the C sources export both names with the same \
             handler and 2 parameters. Send a stateless reply."
                .to_string()
        )]
    );
}

/// A README that is not valid UTF-8 is still harvested. `auth_web3`'s
/// 6.1.5 README spells `©` as the Latin-1 byte 0xA9; the harvester read
/// it with `read_to_string`, which refuses the whole file, so the module
/// and its functions were missing from the 6.1.5 catalogue.
#[test]
fn a_readme_that_is_not_utf8_is_still_harvested() {
    let scratch = common::Scratch::new("kamlsp-harv-latin1");
    let root = scratch.path();
    let readme = root.join("src/modules/latin/README");
    std::fs::create_dir_all(readme.parent().unwrap()).unwrap();
    let mut bytes = b"Latin Module\n\n   Copyright \xa9 2025 Example\n\n".to_vec();
    bytes.extend_from_slice(b"3. Functions\n\n3.1. latin_check(realm)\n\n   Checks the realm.\n");
    std::fs::write(&readme, bytes).unwrap();
    // negative control: a UTF-8 neighbour is harvested either way
    write(
        &root.join("src/modules/plain/README"),
        "Plain Module\n\n3. Functions\n\n3.1. plain_check()\n\n   Checks.\n",
    );
    let mods = harvest_tree(root);
    let names: Vec<(&str, Vec<&str>)> = mods
        .iter()
        .map(|m| {
            (
                m.name.as_str(),
                m.functions.iter().map(|f| f.name.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        names,
        vec![
            ("latin", vec!["latin_check"]),
            ("plain", vec!["plain_check"])
        ]
    );
}
