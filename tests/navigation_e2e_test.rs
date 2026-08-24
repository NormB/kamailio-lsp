//! Workspace symbols, code lenses, quick fixes, and semantic tokens
//! over stdio.

mod common;
use common::*;
use std::process::{Command, Stdio};

const DOC: &str = "route[HELPER] {\n    exit;\n}\nroute[OTHER_THING] {\n    exit;\n}\nfailure_route[HELPER] {\n    drop;\n}\nrequest_route {\n    route(HELPER);\n    route(HELPER);\n}\n";

fn boot(
    tag: &str,
    opts: serde_json::Value,
    text: &str,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
    std::path::PathBuf,
) {
    let base = std::env::temp_dir().join(format!("kamlsp-nav-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let cfg = base.join("t.cfg");
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &base,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "capabilities":{}, "initializationOptions": opts}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "init");
    assert!(
        init["result"]["capabilities"]["workspaceSymbolProvider"]
            .as_bool()
            .unwrap_or(false),
        "workspaceSymbolProvider must be advertised"
    );
    assert!(
        !init["result"]["capabilities"]["codeLensProvider"].is_null(),
        "codeLensProvider must be advertised"
    );
    assert!(
        !init["result"]["capabilities"]["semanticTokensProvider"].is_null(),
        "semanticTokensProvider must be advertised"
    );
    assert!(
        !init["result"]["capabilities"]["codeActionProvider"].is_null(),
        "codeActionProvider must be advertised"
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    (child, rx, stdin, uri, base)
}

#[test]
fn workspace_symbols_filter_across_open_docs() {
    let (mut child, rx, mut stdin, _uri, base) =
        boot("ws", serde_json::json!({"kamailioPath":""}), DOC);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"workspace/symbol","params":{"query":"help"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "wsym");
    let syms = v["result"].as_array().expect("symbols");
    // both the route and the failure_route carry the name
    assert_eq!(syms.len(), 2, "{syms:?}");
    assert!(syms.iter().any(|s| s["name"] == "route[HELPER]"));
    assert!(syms.iter().any(|s| s["name"] == "failure_route[HELPER]"));
    // case-insensitive
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"workspace/symbol","params":{"query":"OTHER_TH"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "wsym-ci");
    assert_eq!(v["result"].as_array().unwrap().len(), 1);
    // empty query returns everything named
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"workspace/symbol","params":{"query":""}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 4, "wsym-all");
    assert!(v["result"].as_array().unwrap().len() >= 3);
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn code_lens_counts_route_references() {
    let (mut child, rx, mut stdin, uri, base) =
        boot("cl", serde_json::json!({"kamailioPath":""}), DOC);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/codeLens","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "codelens");
    let lenses = v["result"].as_array().expect("lenses");
    // only CALLABLE (kind "route") named blocks get a lens: not the
    // anonymous request_route, and NOT the failure_route[HELPER]
    assert_eq!(lenses.len(), 2, "{lenses:?}");
    let helper = lenses
        .iter()
        .find(|l| l["range"]["start"]["line"] == 0)
        .expect("lens on HELPER");
    assert_eq!(helper["command"]["title"], "2 references");
    let other = lenses
        .iter()
        .find(|l| l["range"]["start"]["line"] == 3)
        .expect("lens on OTHER_THING");
    assert_eq!(other["command"]["title"], "0 references");
    assert!(
        !lenses.iter().any(|l| l["range"]["start"]["line"] == 6),
        "failure_route must not carry a reference lens: {lenses:?}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn code_lens_can_be_disabled() {
    let (mut child, rx, mut stdin, uri, base) = boot(
        "cloff",
        serde_json::json!({"kamailioPath":"", "codeLensReferences": false}),
        DOC,
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/codeLens","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "codelens-off");
    assert!(
        v["result"].is_null() || v["result"].as_array().unwrap().is_empty(),
        "no lenses when disabled: {v}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn quick_fix_creates_a_route_stub() {
    let broken = "request_route {\n    route(MISSING);\n}\n";
    let (mut child, rx, mut stdin, uri, base) =
        boot("qf", serde_json::json!({"kamailioPath":""}), broken);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/codeAction","params":{
        "textDocument":{"uri":uri},
        "range":{"start":{"line":1,"character":10},"end":{"line":1,"character":17}},
        "context":{"diagnostics":[{
            "range":{"start":{"line":1,"character":10},"end":{"line":1,"character":17}},
            "severity":2,"source":"kamailio-lsp",
            "message":"route 'MISSING' is not defined here or in included files"}]}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "codeAction");
    let actions = v["result"].as_array().expect("actions");
    assert_eq!(actions.len(), 1, "{actions:?}");
    let a = &actions[0];
    assert_eq!(a["title"], "Create route[MISSING]");
    assert_eq!(a["kind"], "quickfix");
    let edits = a["edit"]["changes"][&uri].as_array().expect("edit");
    assert_eq!(edits[0]["range"]["start"]["line"], 3, "appended at EOF");
    let stub = edits[0]["newText"].as_str().unwrap();
    assert!(stub.contains("route[MISSING]"));
    assert!(stub.contains("exit;"), "kamailio stubs need a body: {stub}");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn quick_fix_loads_the_exporting_module() {
    // the diagnostic position points at the call's closing paren
    // (live 6.0.1 and 6.1.4 shape: "unknown command, missing
    // loadmodule?")
    let base = std::env::temp_dir().join(format!("kamlsp-qflm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let tree = base.join("tree");
    std::fs::create_dir_all(tree.join("src/modules/tm")).unwrap();
    std::fs::write(
        tree.join("src/modules/tm/README"),
        "TM Module\n\n2. Functions\n\n2.1.  t_relay([host, port])\n\n   Relays.\n",
    )
    .unwrap();
    let doc = "loadmodule \"sl.so\"\nrequest_route {\n    t_relay();\n}\n";
    let cfg = base.join("t.cfg");
    std::fs::write(&cfg, doc).unwrap();
    let uri = format!("file://{}", cfg.display());
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env_remove("KAMAILIO_LSP_BIN")
            .env_remove("KAMAILIO_LSP_SRC")
            .env_remove("KAMAILIO_LSP_WIKI")
            .env(
                "KAMAILIO_LSP_CACHE_DIR",
                base.join("cache").display().to_string(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &base,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "capabilities":{},
        "initializationOptions":{"kamailioPath":"","kamailioSrc": tree.display().to_string()}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready")
        },
        "ready",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":doc}}}),
    );
    // position = the `)` of t_relay() on line 2 (0-based col 12)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/codeAction","params":{
        "textDocument":{"uri":uri},
        "range":{"start":{"line":2,"character":12},"end":{"line":2,"character":13}},
        "context":{"diagnostics":[{
            "range":{"start":{"line":2,"character":12},"end":{"line":2,"character":13}},
            "severity":1,"source":"kamailio -c",
            "message":"unknown command, missing loadmodule?"}]}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "codeAction");
    let actions = v["result"].as_array().expect("actions");
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert!(
        actions[0]["title"].as_str().unwrap().contains("tm"),
        "{actions:?}"
    );
    let edits = actions[0]["edit"]["changes"][&uri].as_array().unwrap();
    assert_eq!(edits[0]["range"]["start"]["line"], 1, "after loadmodule");
    assert_eq!(edits[0]["newText"], "loadmodule \"tm.so\"\n");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn semantic_tokens_over_stdio() {
    let (mut child, rx, mut stdin, uri, base) = boot(
        "sem",
        serde_json::json!({"kamailioPath":""}),
        "route[AB] {\n    route(AB);\n}\n",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/semanticTokens/full","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "semanticTokens");
    let data = v["result"]["data"].as_array().expect("data");
    assert_eq!(data.len() % 5, 0);
    let first: Vec<u64> = data[..5].iter().map(|x| x.as_u64().unwrap()).collect();
    assert_eq!(first, vec![0, 6, 2, 0, 0], "route name at line 0 col 6");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}
