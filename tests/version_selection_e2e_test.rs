//! `kamailioVersion` must reach the editor's diagnostics, not just
//! the CLI's.
//!
//! The setting is read in `initialize` and the chosen catalogue is
//! threaded through every publish path by hand. That threading is the
//! part that can silently come undone: make the server ignore the
//! setting and the CLI tests still pass, the unit tests on
//! `catalog_diagnostics` still pass, and an editor quietly checks
//! against a release the user did not ask for.
//!
//! So this asserts the CONSEQUENCE rather than the header.
//! `rr::ignore_user` exists in 5.8.8 and
//! 6.0.7 and was dropped by 4.0: on the newest release it must warn
//! and say where the name still exists, and on 6.0.7 it must be
//! silent, because that release really does export it.

mod common;
use common::*;
use std::cell::RefCell;

thread_local! {
    /// The last `kamailioLsp/catalogue` payload seen by `diagnostics_for`.
    static ANNOUNCED: RefCell<serde_json::Value> = const { RefCell::new(serde_json::Value::Null) };
}
use std::process::{Command, Stdio};

const CFG: &str = "request_route {\n    xlog(\"x\");\n}\nmodparam(\"rr\", \"ignore_user\", 1)\n";

/// Boot with no configured tree, so the built-in catalogue — and the
/// requested release — is what answers.
fn diagnostics_for(tag: &str, version: Option<&str>) -> Vec<String> {
    let base = std::env::temp_dir().join(format!("kamlsp-vsel-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let cfg = base.join("v.cfg");
    std::fs::write(&cfg, CFG).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut opts = serde_json::json!({
        "kamailioPath": "",
        "kamailioSrc": "",
        "kamailioWiki": "",
        "cacheDir": base.join("cache").display().to_string(),
    });
    if let Some(v) = version {
        opts["kamailioVersion"] = serde_json::json!(v);
    }

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", "")
            .env("KAMAILIO_LSP_SRC", "")
            .env("KAMAILIO_LSP_WIKI", "")
            .env("KAMAILIO_LSP_VERSION", "")
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
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    // The server announces what it is judging against before it is
    // ready; the editor shows this so the reader never has to guess.
    // `wait_for` discards what it passes over, so this must be taken
    // before the ready message it precedes.
    let announced = wait_for(
        &rx,
        |v| v["method"] == "kamailioLsp/catalogue",
        "catalogue announcement",
    );
    ANNOUNCED.with(|a| *a.borrow_mut() = announced["params"].clone());

    // Wait for the catalogue to be in place first. Without this the
    // "no diagnostics" case below would be satisfied by a server that
    // simply had not got there yet — an empty result meaning nothing.
    let ready_seen = wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready (")
        },
        "ready",
    );
    let _ = ready_seen;
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":CFG}}}),
    );
    // Drain rather than block: a clean file may publish an empty list
    // or nothing at all, and "nothing" is a real answer here.
    let mut msgs: Vec<String> = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        let Ok(v) = rx.recv_timeout(left.min(std::time::Duration::from_millis(1500))) else {
            break;
        };
        if v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == uri {
            msgs = v["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|d| d["message"].as_str().map(str::to_string))
                .collect();
            break;
        }
    }
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
    msgs
}

#[test]
fn the_editor_checks_against_the_release_the_user_asked_for() {
    let newest = diagnostics_for("newest", None);
    let hit = newest
        .iter()
        .find(|m| m.contains("ignore_user"))
        .unwrap_or_else(|| {
            panic!("the newest release does not export it, so it must warn: {newest:?}")
        });
    assert!(
        hit.contains("6.1.4"),
        "the diagnostic must name the release it judged against: {hit:?}"
    );
    assert!(
        hit.contains("it exists in"),
        "and say where the name still exists: {hit:?}"
    );

    // the consequence: a release that really exports it is silent.
    // A test that only read the header would pass even if choosing a
    // release changed nothing about the answer.
    let older = diagnostics_for("older", Some("6.0.7"));
    assert!(
        !older.iter().any(|m| m.contains("ignore_user")),
        "6.0.7 exports it, so the editor must not warn: {older:?}"
    );
}

/// An unsupported release must not quietly become the newest.
#[test]
fn an_unsupported_release_still_produces_a_working_server() {
    let msgs = diagnostics_for("bogus", Some("9.9.9"));
    let hit = msgs
        .iter()
        .find(|m| m.contains("ignore_user"))
        .unwrap_or_else(|| panic!("the fallback release must still check: {msgs:?}"));
    assert!(
        hit.contains("6.1.4"),
        "it falls back to the newest, and says so in the message: {hit:?}"
    );
}

/// The server tells the client what it is judging against, so the
/// editor can show it without waiting for something to go wrong.
///
/// A warning names the catalogue, but only once a parameter is
/// unknown. Until then nothing tells the reader which release their
/// file is parsed against — and the answer changes with a setting
/// that may have been set for them.
#[test]
fn the_server_announces_the_catalogue_it_uses() {
    // the default: the newest built-in release, named
    let _ = diagnostics_for("announce-default", None);
    let d = ANNOUNCED.with(|a| a.borrow().clone());
    assert_eq!(
        d["version"].as_str(),
        Some("6.1.4"),
        "the newest release must be announced: {d}"
    );
    assert!(
        d["describe"]
            .as_str()
            .unwrap_or_default()
            .contains("Kamailio 6.1.4"),
        "and named as it reads in a sentence: {d}"
    );

    // and it follows the setting, not just the default
    let _ = diagnostics_for("announce-older", Some("6.0.7"));
    let d = ANNOUNCED.with(|a| a.borrow().clone());
    assert_eq!(
        d["version"].as_str(),
        Some("6.0.7"),
        "the announcement must follow the chosen release: {d}"
    );

    // an unsupported one falls back, and announces what it FELL BACK
    // TO rather than what was asked for — the editor must not show a
    // release the server is not using
    let _ = diagnostics_for("announce-bogus", Some("9.9.9"));
    let d = ANNOUNCED.with(|a| a.borrow().clone());
    assert_eq!(
        d["version"].as_str(),
        Some("6.1.4"),
        "the announcement must name what is in use, not what was asked: {d}"
    );
}
