//! Whole-tree harvesting: `harvest_tree` walks the module READMEs of
//! a Kamailio source tree (`src/modules/<name>/README`).

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
    let root = std::env::temp_dir().join(format!("kamlsp-harv-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
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
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn falls_back_to_a_bare_modules_dir() {
    // tolerate being pointed at src/ itself, or at an installed
    // doc tree that has modules/ at the top
    let root = std::env::temp_dir().join(format!("kamlsp-harv-flat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(&root.join("modules/mini/README"), MINI_README);
    let mods = harvest_tree(&root);
    assert_eq!(mods.len(), 1);
    assert_eq!(mods[0].name, "mini");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_tree_harvests_empty() {
    let mods = harvest_tree(std::path::Path::new("/nonexistent/kamailio"));
    assert!(mods.is_empty());
}

#[test]
fn harvests_the_real_kamailio_tree_when_present() {
    // point KAMAILIO_LSP_TEST_TREE at a Kamailio source tree to run
    let Ok(tree) = std::env::var("KAMAILIO_LSP_TEST_TREE") else {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_TREE not set");
        return;
    };
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
    for opt in [
        "kamailioPath",
        "kamailioSrc",
        "kamailioWiki",
        "modulesPath",
        "checkTimeoutMs",
        "maxDiagnostics",
        "snippetCompletions",
        "analyzerDiagnostics",
        "cacheDir",
    ] {
        assert!(
            md.contains(&format!("#### {opt}")),
            "docs/ADMIN.md is missing an option section for {opt}"
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
