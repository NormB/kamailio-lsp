//! The server consults the per-(URI, version) index cache from its
//! hot handlers: several requests at one version build the document
//! index exactly once; a new version builds exactly once more.
//! Observed through the KAMAILIO_LSP_TRACE_INDEX stderr seam.

mod common;
use common::*;
use std::io::Read;
use std::process::{Command, Stdio};

#[test]
fn hot_handlers_share_one_index_build_per_version() {
    let base = std::env::temp_dir().join(format!("kamlsp-memo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let text = "route[A] {\n    exit;\n}\nrequest_route {\n    route(A);\n}\n";
    let cfg = base.join("t.cfg");
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", "")
            .env("KAMAILIO_LSP_SRC", "")
            .env("KAMAILIO_LSP_TRACE_INDEX", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
    // four hot requests at version 1, sequentially fenced by their
    // responses: documentSymbol, semanticTokens full + range, codeLens,
    // references
    for (id, method, extra) in [
        (2, "textDocument/documentSymbol", serde_json::json!({})),
        (3, "textDocument/semanticTokens/full", serde_json::json!({})),
        (
            4,
            "textDocument/semanticTokens/range",
            serde_json::json!({"range":{"start":{"line":0,"character":0},"end":{"line":9,"character":0}}}),
        ),
        (5, "textDocument/codeLens", serde_json::json!({})),
        (
            6,
            "textDocument/references",
            serde_json::json!({"position":{"line":4,"character":11},
                "context":{"includeDeclaration":true}}),
        ),
    ] {
        let mut params = serde_json::json!({"textDocument":{"uri":uri}});
        for (k, v) in extra.as_object().unwrap() {
            params[k] = v.clone();
        }
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        );
        wait_for(&rx, |v| v["id"] == id, method);
    }
    // a new version rebuilds exactly once
    let text2 = "route[A] {\n    exit;\n}\nrequest_route {\n    route(A);\n    exit;\n}\n";
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":text2}]}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":7,"method":"textDocument/documentSymbol","params":{
        "textDocument":{"uri":uri}}}),
    );
    wait_for(&rx, |v| v["id"] == 7, "documentSymbol v2");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":8,"method":"textDocument/codeLens","params":{
        "textDocument":{"uri":uri}}}),
    );
    wait_for(&rx, |v| v["id"] == 8, "codeLens v2");
    child.kill().ok();
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).ok();
    let builds: Vec<&str> = err.lines().filter(|l| l.contains("index build")).collect();
    assert_eq!(
        builds.len(),
        2,
        "one build per version across five hot handlers, got: {builds:?}\nstderr:\n{err}"
    );
    assert!(builds[0].contains("v1"), "{builds:?}");
    assert!(builds[1].contains("v2"), "{builds:?}");
    let _ = std::fs::remove_dir_all(&base);
}
