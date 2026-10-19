//! textDocument/prepareRename and textDocument/documentLink over
//! real stdio LSP.

mod common;
use common::*;
use std::process::{Command, Stdio};

type Started = (
    common::Scratch,
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
);

fn start(text: &str, cfg_name: &str, tag: &str) -> Started {
    start_in(
        common::Scratch::new(&format!("kamlsp-rnl-{tag}")),
        text,
        cfg_name,
    )
}

/// `start` in a scratch directory the caller has already put files in;
/// they stay beside the document while the server reads it.
fn start_in(scratch: common::Scratch, text: &str, cfg_name: &str) -> Started {
    let base = scratch.path().to_path_buf();
    std::fs::create_dir_all(&base).unwrap();
    let cfg = base.join(cfg_name);
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", "")
            .env("KAMAILIO_LSP_SRC", "")
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
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "init");
    // both capabilities must be advertised
    assert_eq!(
        init["result"]["capabilities"]["renameProvider"]["prepareProvider"], true,
        "prepareProvider: {init}"
    );
    assert!(
        init["result"]["capabilities"]["documentLinkProvider"].is_object(),
        "documentLinkProvider: {init}"
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
    (scratch, child, rx, stdin, uri)
}

#[test]
fn prepare_rename_returns_range_and_placeholder_on_a_route_symbol() {
    let text = "route[RELAY] {\n    exit;\n}\nrequest_route {\n    route(RELAY);\n}\n";
    let (_base, mut child, rx, mut stdin, uri) = start(text, "t.cfg", "prep");
    // on the call site name (line 4, inside RELAY)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/prepareRename","params":{
        "textDocument":{"uri":uri},"position":{"line":4,"character":12}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "prepareRename");
    let r = &v["result"];
    assert_eq!(r["placeholder"], "RELAY", "{v}");
    assert_eq!(r["range"]["start"]["line"], 4);
    assert_eq!(r["range"]["start"]["character"], 10);
    assert_eq!(r["range"]["end"]["character"], 15);
    // on the definition name too (line 0)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/prepareRename","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":7}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "prepareRename def");
    assert_eq!(v["result"]["placeholder"], "RELAY", "{v}");
    assert_eq!(v["result"]["range"]["start"]["character"], 6);
    child.kill().ok();
}

#[test]
fn prepare_rename_blocks_off_symbol_and_unrenamable_kinds() {
    let text = "event_route[htable:mod-init] {\n    exit;\n}\nrequest_route {\n    exit;\n}\n";
    let (_base, mut child, rx, mut stdin, uri) = start(text, "t.cfg", "prepblock");
    // off-symbol: whitespace inside a block
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/prepareRename","params":{
        "textDocument":{"uri":uri},"position":{"line":4,"character":1}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "off-symbol");
    assert!(v["result"].is_null(), "off-symbol must be null: {v}");
    // event_route names are module-defined: rename must be blocked at
    // the prepare stage
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/prepareRename","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":15}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "event_route prepare");
    assert!(
        v["result"].is_null(),
        "kind-namespace names are not renamable: {v}"
    );
    child.kill().ok();
}

#[test]
fn document_links_resolve_relative_absolute_and_missing_targets() {
    let scratch = common::Scratch::new("kamlsp-rnl-links");
    let base = scratch.path().to_path_buf();
    std::fs::create_dir_all(base.join("sub")).unwrap();
    std::fs::write(base.join("sub/inc.cfg"), "route[H] { exit; }\n").unwrap();
    let abs = base.join("sub/inc.cfg");
    let text = format!(
        "include_file \"sub/inc.cfg\"\n#!import_file \"{}\"\ninclude_file \"missing.cfg\"\nrequest_route {{ exit; }}\n",
        abs.display()
    );
    let (_scratch, mut child, rx, mut stdin, uri) = start_in(scratch, &text, "main.cfg");
    // the include the relative link names is still on disk beside the
    // document while the server reads it
    assert!(abs.is_file(), "{} is gone", abs.display());
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentLink","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "documentLink");
    let links = v["result"].as_array().expect("links array");
    assert_eq!(links.len(), 3, "{links:?}");
    let abs_uri = format!("file://{}", abs.display());
    // relative resolves against the document's directory
    assert_eq!(links[0]["target"], abs_uri, "{links:?}");
    assert_eq!(links[0]["range"]["start"]["line"], 0);
    assert_eq!(links[0]["range"]["start"]["character"], 14);
    assert_eq!(links[0]["range"]["end"]["character"], 25);
    // absolute passes through
    assert_eq!(links[1]["target"], abs_uri);
    // a missing file still gets a link (the editor surfaces the miss)
    assert_eq!(
        links[2]["target"],
        format!("file://{}", base.join("missing.cfg").display())
    );
    child.kill().ok();
}

#[test]
fn document_links_skip_hostile_paths_and_multibyte_columns_are_utf16() {
    // NUL never links; a multibyte prefix shifts the UTF-16 column
    let text = "include_file \"a\u{0}b.cfg\"\n# émoji 🚀 comment\ninclude_file \"ok.cfg\" # 🚀\n";
    let (_base, mut child, rx, mut stdin, uri) = start(text, "main.cfg", "hostile");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentLink","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "documentLink");
    let links = v["result"].as_array().expect("links array");
    assert_eq!(links.len(), 1, "NUL path must not link: {links:?}");
    assert!(
        links[0]["target"].as_str().unwrap().ends_with("ok.cfg"),
        "{links:?}"
    );
    assert_eq!(links[0]["range"]["start"]["line"], 2);
    assert_eq!(links[0]["range"]["start"]["character"], 14);
    child.kill().ok();
}

/// A link goes where Kamailio opens the file.  `inc/a.cfg` naming
/// `inc/b.cfg` means the main config's `inc/b.cfg` (the checker runs
/// in that directory and tries it first), not `inc/inc/b.cfg` beside
/// the fragment — see `tests/include_order_test.rs`.
#[test]
fn a_link_follows_the_order_kamailio_opens_includes_in() {
    let scratch = common::Scratch::new("kamlsp-rnl-order");
    let base = scratch.path().to_path_buf();
    std::fs::create_dir_all(base.join("inc")).unwrap();
    std::fs::write(base.join("inc/b.cfg"), "route[B] { exit; }\n").unwrap();
    std::fs::write(base.join("main.cfg"), "include_file \"inc/a.cfg\"\n").unwrap();
    let text = "include_file \"inc/b.cfg\"\n";
    std::fs::write(base.join("inc/a.cfg"), text).unwrap();
    let uri = format!("file://{}", base.join("inc/a.cfg").display());
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", "")
            .env("KAMAILIO_LSP_SRC", "")
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
            "workspaceFolders":[{"uri": format!("file://{}", base.display()), "name":"w"}]}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentLink","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "documentLink");
    assert_eq!(
        v["result"][0]["target"],
        format!("file://{}", base.join("inc/b.cfg").display()),
        "{v}"
    );
    child.kill().ok();
}
