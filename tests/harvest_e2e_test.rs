//! Harvest status surfacing:
//!   - workDoneProgress (create/begin/report/end) around the
//!     post-initialize harvest, only when the client advertises
//!     window.workDoneProgress
//!   - a window/showMessage WARNING when a CONFIGURED source or wiki
//!     tree yields zero symbols

mod common;
use common::*;
use std::process::{Command, Stdio};

fn mk_tree(root: &std::path::Path) {
    let readme = root.join("src/modules/mymod/README");
    std::fs::create_dir_all(readme.parent().unwrap()).unwrap();
    std::fs::write(
        readme,
        "MyMod Module\n\n2. Functions\n\n2.1.  my_func(arg)\n\n   Does things.\n",
    )
    .unwrap();
}

fn boot(
    tag: &str,
    capabilities: serde_json::Value,
    opts: serde_json::Value,
) -> (
    std::process::Child,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    std::path::PathBuf,
) {
    let base = std::env::temp_dir().join(format!("kamlsp-harv-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env_remove("KAMAILIO_LSP_BIN")
        .env_remove("KAMAILIO_LSP_SRC")
        .env_remove("KAMAILIO_LSP_WIKI")
        .env_remove("KAMAILIO_LSP_CACHE_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities": capabilities, "initializationOptions": opts}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    (child, rx, stdin, base)
}

fn is_ready(v: &serde_json::Value) -> bool {
    v["method"] == "window/logMessage"
        && v["params"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("ready (")
}

#[test]
fn harvest_sends_work_done_progress_when_supported() {
    let base = std::env::temp_dir().join(format!("kamlsp-harv-prog-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let tree = base.join("tree");
    mk_tree(&tree);
    let (mut child, rx, mut stdin, _b) = boot(
        "prog",
        serde_json::json!({"window":{"workDoneProgress":true}}),
        serde_json::json!({
            "kamailioPath": "",
            "kamailioSrc": tree.display().to_string(),
            "cacheDir": base.join("cache").display().to_string(),
        }),
    );
    // the server asks to create a progress token; answer it
    let create = wait_for(
        &rx,
        |v| v["method"] == "window/workDoneProgress/create",
        "progress create request",
    );
    let token = create["params"]["token"].clone();
    assert!(token.is_string(), "token: {create}");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":create["id"],"result":null}),
    );
    // begin → report → end, all on that token
    let begin = wait_for(
        &rx,
        |v| v["method"] == "$/progress" && v["params"]["value"]["kind"] == "begin",
        "progress begin",
    );
    assert_eq!(begin["params"]["token"], token);
    assert!(
        begin["params"]["value"]["title"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("harvest"),
        "{begin}"
    );
    let report = wait_for(
        &rx,
        |v| v["method"] == "$/progress" && v["params"]["value"]["kind"] == "report",
        "progress report",
    );
    assert_eq!(report["params"]["token"], token);
    let end = wait_for(
        &rx,
        |v| v["method"] == "$/progress" && v["params"]["value"]["kind"] == "end",
        "progress end",
    );
    assert_eq!(end["params"]["token"], token);
    wait_for(&rx, is_ready, "ready");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn no_progress_without_the_client_capability() {
    let base = std::env::temp_dir().join(format!("kamlsp-harv-nocap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let tree = base.join("tree");
    mk_tree(&tree);
    let (mut child, rx, _stdin, _b) = boot(
        "nocap",
        serde_json::json!({}),
        serde_json::json!({
            "kamailioPath": "",
            "kamailioSrc": tree.display().to_string(),
            "cacheDir": base.join("cache").display().to_string(),
        }),
    );
    // the harvest must complete (ready) without ever sending progress
    // or a create request the client cannot handle
    let v = wait_for(
        &rx,
        |v| {
            is_ready(v)
                || v["method"] == "$/progress"
                || v["method"] == "window/workDoneProgress/create"
        },
        "ready without progress",
    );
    assert!(is_ready(&v), "progress sent without the capability: {v}");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn configured_empty_trees_warn_visibly() {
    let base = std::env::temp_dir().join(format!("kamlsp-harv-warn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    // both configured, both yield zero symbols
    let tree = base.join("empty-tree");
    let wiki = base.join("empty-wiki");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::create_dir_all(&wiki).unwrap();
    let (mut child, rx, _stdin, _b) = boot(
        "warn",
        serde_json::json!({}),
        serde_json::json!({
            "kamailioPath": "",
            "kamailioSrc": tree.display().to_string(),
            "kamailioWiki": wiki.display().to_string(),
            "cacheDir": base.join("cache").display().to_string(),
        }),
    );
    let mut warned_src = false;
    let mut warned_wiki = false;
    loop {
        let v = wait_for(
            &rx,
            |v| is_ready(v) || v["method"] == "window/showMessage",
            "warnings then ready",
        );
        if is_ready(&v) {
            break;
        }
        assert_eq!(v["params"]["type"], 2, "WARNING severity: {v}");
        let msg = v["params"]["message"].as_str().unwrap();
        if msg.contains(&tree.display().to_string()) {
            warned_src = true;
        }
        if msg.contains(&wiki.display().to_string()) {
            warned_wiki = true;
        }
    }
    assert!(
        warned_src,
        "zero-module source tree must warn with its path"
    );
    assert!(warned_wiki, "zero-symbol wiki tree must warn with its path");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn unconfigured_trees_stay_silent() {
    // the unset default must neither warn nor send progress
    let (mut child, rx, stdin, base) = boot(
        "silent",
        serde_json::json!({"window":{"workDoneProgress":true}}),
        serde_json::json!({"kamailioPath": ""}),
    );
    let v = wait_for(
        &rx,
        |v| {
            is_ready(v)
                || v["method"] == "window/showMessage"
                || v["method"] == "$/progress"
                || v["method"] == "window/workDoneProgress/create"
        },
        "silent ready",
    );
    assert!(is_ready(&v), "unconfigured harvest must stay silent: {v}");
    drop(stdin);
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}
