//! Server-side settings:
//!   - snippetCompletions: function completions insert tabstop
//!     snippets (default on; false = plain text)
//!   - maxDiagnostics: bound on published diagnostics per file
//!   - cacheDir: catalog cache location as an initialization option

mod common;
use common::Server;
use std::process::{Command, Stdio};

fn mk_tree(root: &std::path::Path) {
    let w = |p: std::path::PathBuf, c: &str| {
        std::fs::create_dir_all(p.parent().expect("the path has a parent directory"))
            .expect("the test can create its directory");
        std::fs::write(p, c).expect("the test can write its file");
    };
    w(
        root.join("src/modules/mymod/README"),
        "MyMod Module\n\n2. Functions\n\n2.1.  my_func(arg)\n\n   Does things.\n",
    );
}

fn boot(
    tag: &str,
    extra_opts: serde_json::Value,
    stub_errors: usize,
) -> (
    common::Scratch,
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
) {
    let scratch = common::Scratch::new(&format!("kamlsp-set-{tag}"));
    let base = scratch.path().to_path_buf();
    let tree = base.join("tree");
    mk_tree(&tree);
    let cfg = base.join("t.cfg");
    std::fs::write(&cfg, "loadmodule \"mymod.so\"\nrequest_route { exit; }\n")
        .expect("the test can write its file");
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
    std::fs::write(&stub, lines).expect("the test can write its file");
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub)
        .expect("the stub was just written")
        .permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).expect("the test can make its stub executable");

    let mut opts = serde_json::json!({
        "kamailioPath": if stub_errors > 0 { stub.display().to_string() } else { String::new() },
        "kamailioSrc": tree.display().to_string(),
        "cacheDir": base.join("cache").display().to_string(),
    });
    for (k, v) in extra_opts
        .as_object()
        .expect("the extra options are a JSON object")
    {
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
            .expect("the server binary starts"),
        &base,
    );
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child
        .stdin
        .take()
        .expect("the server was spawned with a piped stdin");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{}, "initializationOptions": opts}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::wait_for(
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
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,
                "text":"loadmodule \"mymod.so\"\nrequest_route { exit; }\n"}}}),
    );
    (scratch, child, rx, stdin, uri)
}

fn completion_item<'a>(v: &'a serde_json::Value, label: &str) -> Option<&'a serde_json::Value> {
    v["result"]
        .as_array()
        .expect("the field is a JSON array")
        .iter()
        .find(|i| i["label"] == label)
}

#[test]
fn function_completions_are_snippets_by_default() {
    let (_base, mut child, rx, mut stdin, uri) = boot("snip", serde_json::json!({}), 0);
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":1,"character":18}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 2, "completion");
    let item = completion_item(&v, "my_func").expect("my_func offered");
    assert_eq!(item["insertTextFormat"], 2, "snippet format: {item}");
    assert_eq!(
        item["insertText"], "my_func($1)$0",
        "tabstop snippet: {item}"
    );
    // keywords stay plain
    let kw = completion_item(&v, "exit").expect("keyword");
    assert!(kw.get("insertText").is_none(), "keywords stay plain: {kw}");
    child.kill().ok();
}

#[test]
fn snippet_completions_can_be_disabled() {
    let (_base, mut child, rx, mut stdin, uri) = boot(
        "nosnip",
        serde_json::json!({"snippetCompletions": false}),
        0,
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":1,"character":18}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 2, "completion");
    let item = completion_item(&v, "my_func").expect("my_func offered");
    assert!(
        item.get("insertTextFormat").is_none() && item.get("insertText").is_none(),
        "plain completion when disabled: {item}"
    );
    child.kill().ok();
}

#[test]
fn max_diagnostics_bounds_the_publish() {
    let (_base, mut child, rx, _stdin, _uri) =
        boot("maxdiag", serde_json::json!({"maxDiagnostics": 2}), 5);
    let d = common::wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publish",
    );
    assert_eq!(
        d["params"]["diagnostics"].as_array().unwrap().len(),
        2,
        "5 planted, capped at 2: {d}"
    );
    child.kill().ok();
}

#[test]
fn cache_dir_option_is_honored() {
    let (base, mut child, _rx, _stdin, _uri) = boot("cachedir", serde_json::json!({}), 0);
    // the harvest above must have written the cache into the OPTION path
    let cache = base.join("cache");
    let n = std::fs::read_dir(&cache).map(|d| d.count()).unwrap_or(0);
    assert!(n >= 1, "cacheDir option ignored: {} entries", n);
    child.kill().ok();
}
