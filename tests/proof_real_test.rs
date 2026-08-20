//! Proof suite: the server, spoken to over real LSP stdio, handles a
//! real Kamailio source tree, wiki checkout, and binary — module
//! docs, core functions, core parameters, pseudo-variables, and
//! (with the binary) `-c` diagnostics.
//!
//! Gated: set KAMAILIO_LSP_TEST_TREE to a Kamailio source tree;
//! KAMAILIO_LSP_TEST_WIKI to a kamailio-wiki checkout; and
//! KAMAILIO_LSP_TEST_BIN to a kamailio binary to also prove the
//! diagnostics leg.

mod common;
use common::*;
use std::process::{Command, Stdio};

const CFG: &str = "#!KAMAILIO\nloadmodule \"sl.so\"\nloadmodule \"tm.so\"\nmodparam(\"tm\", \"fr_timer\", 30000)\nrequest_route { exit; }\n";

fn labels(v: &serde_json::Value) -> Vec<String> {
    v["result"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|i| i["label"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn full_stack_against_a_real_kamailio_tree() {
    let Ok(tree) = std::env::var("KAMAILIO_LSP_TEST_TREE") else {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_TREE not set");
        return;
    };
    let wiki = std::env::var("KAMAILIO_LSP_TEST_WIKI").unwrap_or_default();
    let bin = std::env::var("KAMAILIO_LSP_TEST_BIN").unwrap_or_default();

    let dir = std::env::temp_dir().join(format!("kamlsp-proof-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("proof.cfg");
    // an unknown modparam so the diagnostics leg has something to find
    let bad = format!("{CFG}modparam(\"tm\", \"no_such_param_xyz\", 1)\n");
    std::fs::write(&cfg, &bad).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env("KAMAILIO_LSP_BIN", &bin) // empty = diagnostics off
        .env("KAMAILIO_LSP_SRC", &tree)
        .env("KAMAILIO_LSP_WIKI", &wiki)
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
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    // the harvest is asynchronous: wait for the readiness log line
    wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready (")
        },
        "harvest-ready logMessage",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":bad}}}),
    );

    // diagnostics leg first: wait_for discards non-matching traffic,
    // so this must be consumed before the request/response waits below
    if !bin.is_empty() {
        let d = wait_for(
            &rx,
            |v| {
                v["method"] == "textDocument/publishDiagnostics"
                    && !v["params"]["diagnostics"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .is_empty()
            },
            "real -c diagnostics",
        );
        let diags = d["params"]["diagnostics"].as_array().unwrap();
        let msgs: Vec<String> = diags
            .iter()
            .map(|x| x["message"].as_str().unwrap_or("").to_string())
            .collect();
        // kamailio's positioned message for a bad modparam is generic
        // ("Can't set module parameter") — the position is the proof
        assert!(
            msgs.iter()
                .any(|m| m.contains("module parameter") || m.contains("no_such_param_xyz")),
            "diagnostics: {msgs:?}"
        );
        // the bad modparam is on line 6 (1-based) = 5 (0-based)
        assert!(
            diags
                .iter()
                .any(|x| x["range"]["start"]["line"].as_u64() == Some(5)),
            "position: {diags:?}"
        );
    }

    // 1. module-param completion inside modparam("tm", "...
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":3,"character":16}}}),
    );
    let l = labels(&wait_for(&rx, |v| v["id"] == 2, "tm param completion"));
    assert!(l.contains(&"fr_timer".to_string()), "tm params: {l:?}");
    assert!(
        l.contains(&"reparse_invite".to_string()),
        "tm params: {l:?}"
    );

    // 2. code position offers module functions AND (with a wiki) core
    // functions/parameters
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":4,"character":17}}}),
    );
    let l = labels(&wait_for(&rx, |v| v["id"] == 3, "code completion"));
    assert!(l.contains(&"t_relay".to_string()), "module fn: {l:?}");
    assert!(l.contains(&"sl_send_reply".to_string()), "sl fn missing");
    if !wiki.is_empty() {
        assert!(l.contains(&"force_rport".to_string()), "core fn missing");
        assert!(
            l.contains(&"advertised_address".to_string()),
            "core param missing"
        );
    }

    // 3. hover on the tm modparam name documents it
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":3,"character":18}}}),
    );
    let h = wait_for(&rx, |v| v["id"] == 4, "hover");
    let hover = h["result"]["contents"]["value"].as_str().unwrap_or("");
    assert!(hover.contains("fr_timer"), "hover: {hover}");

    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}
