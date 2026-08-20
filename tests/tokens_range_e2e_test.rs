//! textDocument/semanticTokens/range over real stdio LSP.

mod common;
use common::*;
use std::process::{Command, Stdio};

#[test]
fn semantic_tokens_range_returns_the_slice_with_fresh_deltas() {
    let base = std::env::temp_dir().join(format!("kamlsp-str-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let text =
        "route[A] {\n    exit;\n}\nroute[B] {\n    route(B);\n}\nroute[C] {\n    route(A);\n}\n";
    let cfg = base.join("t.cfg");
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env("KAMAILIO_LSP_BIN", "")
        .env("KAMAILIO_LSP_SRC", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "init");
    let sem = &init["result"]["capabilities"]["semanticTokensProvider"];
    assert_eq!(sem["range"], true, "range capability: {init}");
    assert_eq!(sem["full"], true, "full stays advertised: {init}");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    // the middle route only (lines 3..6)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/semanticTokens/range","params":{
        "textDocument":{"uri":uri},
        "range":{"start":{"line":3,"character":0},"end":{"line":6,"character":0}}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "range tokens");
    let data: Vec<u64> = v["result"]["data"]
        .as_array()
        .expect("data")
        .iter()
        .map(|n| n.as_u64().unwrap())
        .collect();
    // exactly B's definition and call, first delta document-absolute
    assert_eq!(data, vec![3, 6, 1, 0, 0, 1, 10, 1, 0, 0], "{v}");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}
