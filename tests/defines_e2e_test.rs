//! Preprocessor symbols in the editor: completion, hover, go to
//! definition and the outline.
//!
//! `#!define` binds a name the preprocessor substitutes textually, so
//! a use is a bare identifier anywhere — including inside `#!ifdef`,
//! which is why none of this is gated on code position.

mod common;
use common::*;
use std::process::{Command, Stdio};

const CFG: &str = "\
#!KAMAILIO
#!define WITH_TLS 1
#!define RELAY MYROUTE
#!substdef \"!MYPORT!5060!g\"
request_route {
#!ifdef WITH_TLS
    route(RELAY);
#!endif
    xlog(\"MYPORT\");
}
route[MYROUTE] {
    exit;
}
";

fn boot(
    tag: &str,
    text: &str,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
) {
    let dir = std::env::temp_dir().join(format!("kamlsp-def-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("t.cfg");
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
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    (child, rx, stdin, uri)
}

#[test]
fn a_define_name_jumps_to_its_directive() {
    let (mut child, rx, mut stdin, uri) = boot("goto", CFG);
    // cursor on RELAY inside `route(RELAY);` (line 6)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{
            "textDocument":{"uri":uri},"position":{"line":6,"character":11}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 2, "definition");
    // RELAY is a define, so it resolves to the #!define line (2), not
    // to route[MYROUTE]
    assert_eq!(
        r["result"]["range"]["start"]["line"], 2,
        "expected the #!define line: {}",
        r["result"]
    );
    let _ = child.kill();
}

#[test]
fn a_define_inside_ifdef_jumps_too() {
    let (mut child, rx, mut stdin, uri) = boot("ifdef", CFG);
    // cursor on WITH_TLS inside `#!ifdef WITH_TLS` (line 5) — a
    // directive operand, not code position
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{
            "textDocument":{"uri":uri},"position":{"line":5,"character":10}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 2, "definition");
    assert_eq!(
        r["result"]["range"]["start"]["line"], 1,
        "expected the #!define WITH_TLS line: {}",
        r["result"]
    );
    let _ = child.kill();
}

#[test]
fn hover_shows_what_a_define_binds() {
    let (mut child, rx, mut stdin, uri) = boot("hover", CFG);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":6,"character":11}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 2, "hover");
    let md = r["result"]["contents"]["value"].as_str().unwrap_or("");
    assert!(md.contains("RELAY"), "hover: {md}");
    assert!(md.contains("MYROUTE"), "the bound value must show: {md}");
    assert!(md.contains("define"), "the directive must show: {md}");
    let _ = child.kill();
}

#[test]
fn a_substdef_name_is_a_symbol_like_any_other() {
    let (mut child, rx, mut stdin, uri) = boot("substdef", CFG);
    // MYPORT is bound by #!substdef, used inside a string
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":8,"character":11}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 2, "hover");
    let md = r["result"]["contents"]["value"].as_str().unwrap_or("");
    assert!(md.contains("MYPORT") && md.contains("5060"), "hover: {md}");
    let _ = child.kill();
}

#[test]
fn defines_complete_in_code_position() {
    let (mut child, rx, mut stdin, uri) = boot("complete", CFG);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":8,"character":4}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 2, "completion");
    let empty = vec![];
    let labels: Vec<&str> = r["result"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|i| i["label"].as_str().unwrap_or(""))
        .collect();
    for want in ["WITH_TLS", "RELAY", "MYPORT"] {
        assert!(labels.contains(&want), "{want} missing from completion");
    }
    let _ = child.kill();
}

#[test]
fn defines_appear_in_the_outline() {
    let (mut child, rx, mut stdin, uri) = boot("outline", CFG);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{
            "textDocument":{"uri":uri}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 2, "documentSymbol");
    let empty = vec![];
    let names: Vec<&str> = r["result"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .map(|s| s["name"].as_str().unwrap_or(""))
        .collect();
    for want in ["WITH_TLS", "RELAY", "MYPORT"] {
        assert!(
            names.contains(&want),
            "{want} missing from the outline: {names:?}"
        );
    }
    // and the route blocks are still there
    assert!(names.iter().any(|n| n.contains("MYROUTE")), "{names:?}");
    let _ = child.kill();
}
