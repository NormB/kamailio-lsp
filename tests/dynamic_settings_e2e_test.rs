//! Dynamic settings: workspace/didChangeConfiguration retunes the
//! runtime toggles (analyzer diagnostics, snippet completions, code
//! lens references, maxDiagnostics, checkTimeoutMs) without a server
//! restart, republishing diagnostics for open documents.

mod common;
use common::*;
use std::process::{Command, Stdio};

/// Boot a server: optional stub checker planting `stub_errors`
/// parse errors, a one-module source tree, and `opts` merged over
/// the defaults.  Opens t.cfg with `text`.
fn boot(
    tag: &str,
    extra_opts: serde_json::Value,
    stub_errors: usize,
    text: &str,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
    std::path::PathBuf,
) {
    let base = std::env::temp_dir().join(format!("kamlsp-dyn-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let tree = base.join("tree");
    let readme = tree.join("src/modules/mymod/README");
    std::fs::create_dir_all(readme.parent().unwrap()).unwrap();
    std::fs::write(
        readme,
        "MyMod Module\n\n2. Functions\n\n2.1.  my_func(arg)\n\n   Does things.\n",
    )
    .unwrap();
    let cfg = base.join("t.cfg");
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let stub = base.join("stub.sh");
    let mut lines = String::from("#!/bin/sh\ncfg=\"\"\nfor a in \"$@\"; do cfg=\"$a\"; done\n");
    for i in 0..stub_errors {
        lines.push_str(&format!(
            "echo \" 0(1) CRITICAL: <core> [core/cfg.y:1]: yyerror_at(): parse error in config file $cfg, line 1, column {}-{}: planted {i}\" >&2\n",
            i + 1,
            i + 2
        ));
    }
    lines.push_str("exit 255\n");
    std::fs::write(&stub, lines).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();

    let mut opts = serde_json::json!({
        "kamailioPath": if stub_errors > 0 { stub.display().to_string() } else { String::new() },
        "kamailioSrc": tree.display().to_string(),
        "cacheDir": base.join("cache").display().to_string(),
    });
    for (k, v) in extra_opts.as_object().unwrap() {
        opts[k] = v.clone();
    }

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env_remove("KAMAILIO_LSP_BIN")
            .env_remove("KAMAILIO_LSP_SRC")
            .env_remove("KAMAILIO_LSP_WIKI")
            .env_remove("KAMAILIO_LSP_CACHE_DIR")
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
                    .contains("ready (")
        },
        "ready",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":text}}}),
    );
    (child, rx, stdin, uri, base)
}

fn change_config(stdin: &mut std::process::ChildStdin, settings: serde_json::Value) {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration",
            "params":{"settings": settings}}),
    );
}

#[test]
fn max_diagnostics_applies_without_restart() {
    let (mut child, rx, mut stdin, _uri, base) = boot(
        "maxdiag",
        serde_json::json!({}),
        5,
        "loadmodule \"mymod.so\"\nrequest_route { exit; }\n",
    );
    let d = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "initial publish",
    );
    assert_eq!(
        d["params"]["diagnostics"].as_array().unwrap().len(),
        5,
        "all planted errors before the change: {d}"
    );
    change_config(
        &mut stdin,
        serde_json::json!({"kamailioLsp": {"maxDiagnostics": 2}}),
    );
    let d = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|a| a.len() == 2)
        },
        "capped republish",
    );
    assert_eq!(d["params"]["diagnostics"].as_array().unwrap().len(), 2);
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn analyzer_toggle_silences_and_restores_diags() {
    // no checker: the analyzer diag (undefined route) is the signal
    let (mut child, rx, mut stdin, _uri, base) = boot(
        "antoggle",
        serde_json::json!({}),
        0,
        "request_route {\n    route(GONE);\n}\n",
    );
    let d = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && !v["params"]["diagnostics"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .is_empty()
        },
        "analyzer diag",
    );
    assert!(
        d["params"]["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("GONE")
    );
    // OFF: the next publish for this doc must be empty
    change_config(
        &mut stdin,
        serde_json::json!({"kamailioLsp": {"analyzerDiagnostics": false}}),
    );
    let d = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|a| a.is_empty())
        },
        "silenced republish",
    );
    assert!(d["params"]["diagnostics"].as_array().unwrap().is_empty());
    // ON again: the diag comes back without any edit or save
    change_config(
        &mut stdin,
        serde_json::json!({"kamailioLsp": {"analyzerDiagnostics": true}}),
    );
    let d = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && !v["params"]["diagnostics"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .is_empty()
        },
        "restored republish",
    );
    assert!(
        d["params"]["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("GONE")
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn snippet_toggle_applies_to_the_next_completion() {
    let (mut child, rx, mut stdin, uri, base) = boot(
        "sniptoggle",
        serde_json::json!({}),
        0,
        "loadmodule \"mymod.so\"\nrequest_route { exit; }\n",
    );
    change_config(
        &mut stdin,
        serde_json::json!({"kamailioLsp": {"snippetCompletions": false}}),
    );
    // fence: a round-trip guarantees the notification was processed
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":1,"character":18}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "completion");
    let item = v["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "my_func")
        .expect("my_func offered")
        .clone();
    assert!(
        item.get("insertText").is_none() && item.get("insertTextFormat").is_none(),
        "snippets disabled dynamically: {item}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn flat_settings_and_garbage_are_tolerated() {
    let (mut child, rx, mut stdin, uri, base) = boot(
        "garbage",
        serde_json::json!({}),
        5,
        "loadmodule \"mymod.so\"\nrequest_route { exit; }\n",
    );
    wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "initial publish",
    );
    // hostile shapes: none may crash the server
    for garbage in [
        serde_json::json!(null),
        serde_json::json!("just a string"),
        serde_json::json!({"kamailioLsp": null}),
        serde_json::json!({"kamailioLsp": {"maxDiagnostics": "NaN\u{0}\\"}}),
        serde_json::json!({"kamailioLsp": {"maxDiagnostics": 0}}),
        serde_json::json!({"analyzerDiagnostics": []}),
    ] {
        change_config(&mut stdin, garbage);
    }
    // FLAT settings (no kamailioLsp wrapper) must also work
    change_config(&mut stdin, serde_json::json!({"maxDiagnostics": 2}));
    let d = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|a| a.len() == 2)
        },
        "flat-shape capped republish",
    );
    assert_eq!(d["params"]["diagnostics"].as_array().unwrap().len(), 2);
    // server still answers requests after the garbage barrage
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":0,"character":1}}}),
    );
    wait_for(&rx, |v| v["id"] == 3, "hover after garbage");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}
