//! Include-awareness + analyzer diagnostics over real stdio LSP:
//! cross-file definition, include-aware completion, and fast
//! (no-save, debounced) analyzer diagnostics.

mod common;
use common::Server;
use std::process::{Command, Stdio};

fn setup(base: &std::path::Path) -> (String, String) {
    std::fs::create_dir_all(base).expect("the test can create its directory");
    std::fs::write(base.join("inc.cfg"), "route[HELPER] {\n    exit;\n}\n")
        .expect("the test can write its file");
    let main_text = "include_file \"inc.cfg\"\nrequest_route {\n    route(HELPER);\n}\n";
    let main = base.join("main.cfg");
    std::fs::write(&main, main_text).expect("the test can write its file");
    (format!("file://{}", main.display()), main_text.to_string())
}

fn start(
    base: &std::path::Path,
    init_opts: serde_json::Value,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
) {
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", "")
            .env("KAMAILIO_LSP_ANALYZER_DEBOUNCE_MS", "10")
            .env(
                "KAMAILIO_LSP_CACHE_DIR",
                base.join("cache").display().to_string(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the server binary starts"),
        base,
    );
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child
        .stdin
        .take()
        .expect("the server was spawned with a piped stdin");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "capabilities":{}, "initializationOptions": init_opts}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    (child, rx, stdin)
}

#[test]
fn cross_file_definition_and_route_completion() {
    let scratch = common::Scratch::new("kamlsp-inc");
    let base = scratch.path().to_path_buf();
    let (uri, text) = setup(&base);
    let (mut child, rx, mut stdin) = start(&base, serde_json::json!({"kamailioPath":""}));
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    // definition of `HELPER` (line 2, inside the name) → inc.cfg
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{
        "textDocument":{"uri":uri},"position":{"line":2,"character":11}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 2, "definition");
    let target = v["result"]["uri"].as_str().expect("cross-file location");
    assert!(
        target.ends_with("inc.cfg"),
        "definition must be in the include: {v}"
    );
    assert_eq!(v["result"]["range"]["start"]["line"], 0);
    // route( completion offers the include's route
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{
        "textDocument":{"uri":uri},"position":{"line":2,"character":10}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 3, "completion");
    let labels: Vec<&str> = v["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(
        labels.contains(&"HELPER"),
        "include-defined route completes: {labels:?}"
    );
    child.kill().ok();
}

#[test]
fn analyzer_diags_appear_on_change_without_save() {
    let scratch = common::Scratch::new("kamlsp-and");
    let base = scratch.path().to_path_buf();
    let (uri, text) = setup(&base);
    let (mut child, rx, mut stdin) = start(&base, serde_json::json!({"kamailioPath":""}));
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    // break the ref WITHOUT saving: analyzer diag must arrive
    let broken = "include_file \"inc.cfg\"\nrequest_route {\n    route(GONE);\n}\n";
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":broken}]}}),
    );
    let diag = common::wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && !v["params"]["diagnostics"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .is_empty()
        },
        "analyzer diagnostics",
    );
    let ds = diag["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(ds[0]["message"].as_str().unwrap().contains("GONE"));
    assert_eq!(ds[0]["source"], "kamailio-lsp");
    assert_eq!(ds[0]["severity"], 2, "warning");
    assert_eq!(ds[0]["range"]["start"]["line"], 2);
    assert_eq!(diag["params"]["version"], 2);
    // fix it again: diags must clear
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":3},
        "contentChanges":[{"text":text}]}}),
    );
    let clear = common::wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["version"] == 3,
        "clear",
    );
    assert!(
        clear["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    child.kill().ok();
}

#[test]
fn analyzer_diags_can_be_disabled() {
    let scratch = common::Scratch::new("kamlsp-anoff");
    let base = scratch.path().to_path_buf();
    let (uri, _) = setup(&base);
    let (mut child, rx, mut stdin) = start(
        &base,
        serde_json::json!({"kamailioPath":"", "analyzerDiagnostics": false}),
    );
    let broken = "request_route {\n    route(GONE);\n}\n";
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":broken}}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":broken.replace("GONE", "ALSO_GONE")}]}}),
    );
    // bounded negative check: a hover round-trip after the debounce
    // window; no non-empty publish may precede its response
    std::thread::sleep(std::time::Duration::from_millis(150));
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":9,"method":"textDocument/hover","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":1}}}),
    );
    loop {
        let v = common::wait_for(
            &rx,
            |v| v["id"] == 9 || v["method"] == "textDocument/publishDiagnostics",
            "hover or diags",
        );
        if v["id"] == 9 {
            break;
        }
        assert!(
            v["params"]["diagnostics"].as_array().unwrap().is_empty(),
            "no analyzer diagnostics when disabled: {v}"
        );
    }
    child.kill().ok();
}

#[test]
fn references_and_rename_cross_the_include_closure() {
    let scratch = common::Scratch::new("kamlsp-xref");
    let base = scratch.path().to_path_buf();
    let (uri, text) = setup(&base);
    let inc_uri = format!("file://{}", base.join("inc.cfg").display());
    let (mut child, rx, mut stdin) = start(&base, serde_json::json!({"kamailioPath":""}));
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    // references from the call in main.cfg include the DEF in inc.cfg
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{
        "textDocument":{"uri":uri},"position":{"line":2,"character":11},
        "context":{"includeDeclaration":true}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 2, "references");
    let locs = v["result"].as_array().expect("locations");
    assert_eq!(locs.len(), 2, "call in main + def in include: {locs:?}");
    assert!(
        locs.iter().any(|l| l["uri"] == inc_uri),
        "the include's definition must be listed: {locs:?}"
    );
    // rename rewrites BOTH files
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/rename","params":{
        "textDocument":{"uri":uri},"position":{"line":2,"character":11},"newName":"HELPER2"}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 3, "rename");
    let changes = v["result"]["changes"].as_object().expect("changes");
    assert_eq!(changes.len(), 2, "edits in both files: {changes:?}");
    assert_eq!(changes[&uri].as_array().unwrap().len(), 1);
    assert_eq!(changes[&inc_uri].as_array().unwrap().len(), 1);
    let inc_edit = &changes[&inc_uri][0];
    assert_eq!(inc_edit["newText"], "HELPER2");
    assert_eq!(inc_edit["range"]["start"]["line"], 0);
    child.kill().ok();
}

/// The editor shows the same warning: a root whose include closure
/// stopped at the 500-file bound carries it on its first line.
#[test]
fn a_root_whose_closure_stops_at_the_bound_is_told_so() {
    let base_scratch = common::Scratch::new("kamlsp-inc-bound");
    let base = base_scratch.path().to_path_buf();
    std::fs::create_dir_all(base.join("inc")).unwrap();
    let mut text = String::new();
    for i in 0..500 {
        text.push_str(&format!("include_file \"inc/c{i}.cfg\"\n"));
        std::fs::write(
            base.join(format!("inc/c{i}.cfg")),
            "route[x] {\n    exit;\n}\n",
        )
        .unwrap();
    }
    let main = base.join("kamailio.cfg");
    std::fs::write(&main, &text).unwrap();
    let uri = format!("file://{}", main.display());
    let (mut child, rx, mut stdin) = start(&base, serde_json::json!({"kamailioPath":""}));
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    let v = common::wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == uri,
        "diagnostics on the root",
    );
    let hit = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| {
            d["message"]
                .as_str()
                .is_some_and(|m| m.starts_with("the include closure stopped at 500 files"))
        })
        .cloned();
    let d = hit.unwrap_or_else(|| panic!("no bound warning: {v}"));
    assert_eq!(d["range"]["start"]["line"], 0, "{d}");
    let _ = child.kill();
}

/// Completion inside `include_file "inc/` offers the files in `inc/`,
/// and the edit replaces only the part typed after the last `/`.
#[test]
fn include_paths_complete_inside_the_quotes() {
    let base_scratch = common::Scratch::new("kamlsp-inc-paths");
    let base = base_scratch.path().to_path_buf();
    std::fs::create_dir_all(base.join("inc/routes")).unwrap();
    std::fs::write(base.join("inc/globals.cfg"), "debug = 2\n").unwrap();
    std::fs::write(base.join("inc/notes.txt"), "not a config\n").unwrap();
    let text = "include_file \"inc/gl\"\n";
    let main = base.join("kamailio.cfg");
    std::fs::write(&main, text).unwrap();
    let uri = format!("file://{}", main.display());
    let (mut child, rx, mut stdin) = start(&base, serde_json::json!({"kamailioPath":""}));
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    // the cursor after `inc/gl`
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":20}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 2, "completion");
    let items = v["result"].as_array().unwrap_or_else(|| panic!("{v}"));
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    assert_eq!(labels, vec!["globals.cfg", "routes/"], "{v}");
    let globals = &items[0];
    assert_eq!(globals["kind"], 17, "CompletionItemKind.File: {globals}");
    assert_eq!(items[1]["kind"], 19, "CompletionItemKind.Folder");
    assert_eq!(
        globals["textEdit"]["range"],
        serde_json::json!({"start":{"line":0,"character":18},"end":{"line":0,"character":20}}),
        "only `gl` is replaced: {globals}"
    );
    assert_eq!(globals["textEdit"]["newText"], "globals.cfg");
    let _ = child.kill();
}
