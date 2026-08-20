//! End-to-end: spawn the real server binary and speak LSP over stdio.
//! A stub `kamailio` binary supplies deterministic -c output.

mod common;
use common::*;
use std::process::{Command, Stdio};
use std::time::Duration;

/// A stub must find the cfg path as the LAST argument: the server
/// invokes `kamailio -c --all-errors -Y <dir> -f <file>`.
const STUB_PREAMBLE: &str = "#!/bin/sh\ncfg=\"\"\nfor a in \"$@\"; do cfg=\"$a\"; done\n";

#[test]
fn diagnostics_flow_over_stdio() {
    let dir = std::env::temp_dir().join(format!("kamlsp-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // deterministic stub standing in for the kamailio binary: one
    // range error and one spanning error, in the real 6.0.1 shapes
    let stub = dir.join("kamailio-stub.sh");
    std::fs::write(
        &stub,
        format!(
            "{STUB_PREAMBLE}echo \" 0(1) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file $cfg, line 2, column 5-7: stub says no\" >&2\necho \" 0(1) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file $cfg, from line 1, column 1 to line 2, column 3: stub span\" >&2\nexit 255\n"
        ),
    )
    .unwrap();
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();

    let cfg = dir.join("test.cfg");
    std::fs::write(&cfg, "loadmodule \"nope.so\"\nbroken here\n").unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env("KAMAILIO_LSP_BIN", stub.display().to_string())
        .env("KAMAILIO_LSP_SRC", "") // no catalog needed for this flow
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("server binary must spawn");
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"capabilities":{}}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "initialize result");
    assert!(
        init["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap_or(false)
    );

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,
                "text":"loadmodule \"nope.so\"\nbroken here\n"}}}),
    );

    let diag = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publishDiagnostics",
    );
    let ds = diag["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(ds.len(), 2, "both stub diagnostics: {ds:?}");
    assert_eq!(ds[0]["message"], "stub says no");
    assert_eq!(ds[0]["range"]["start"]["line"], 1);
    assert_eq!(ds[0]["range"]["start"]["character"], 4);
    assert_eq!(ds[0]["range"]["end"]["character"], 7);
    assert_eq!(ds[0]["source"], "kamailio -c");
    // the spanning error covers two lines
    assert_eq!(ds[1]["message"], "stub span");
    assert_eq!(ds[1]["range"]["start"]["line"], 0);
    assert_eq!(ds[1]["range"]["start"]["character"], 0);
    assert_eq!(ds[1]["range"]["end"]["line"], 1);

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":9,"method":"shutdown"}),
    );
    wait_for(&rx, |v| v["id"] == 9, "shutdown result");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
    );
    // real clients close the pipe after `exit`; tower-lsp's serve loop
    // terminates on stdin EOF
    drop(stdin);
    // bounded wait: a server that ignores `exit` must fail the test,
    // not hang it
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(st) = child.try_wait().expect("try_wait") {
            break st;
        }
        if std::time::Instant::now() > deadline {
            child.kill().ok();
            panic!("server did not exit within 10s of the exit notification");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "clean exit, got {status:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_invocation_carries_the_kamailio_flags() {
    // the stub records its argv: the server must pass -c,
    // --all-errors, -Y <writable dir>, and -f <file>; with
    // modulesPath set, -L <dir> too
    let dir = std::env::temp_dir().join(format!("kamlsp-e2e-args-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let argv_file = dir.join("argv");
    let stub = dir.join("kamailio-stub.sh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nexit 0\n",
            argv_file.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, "request_route { exit; }\n").unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env_remove("KAMAILIO_LSP_BIN")
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
        "capabilities":{},
        "initializationOptions":{
            "kamailioPath": stub.display().to_string(),
            "modulesPath": "/opt/kamailio/modules"
        }}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,
                "text":"request_route { exit; }\n"}}}),
    );
    // rc=0, no diagnostics: the empty publish is the fence
    wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publish fence",
    );
    let argv = std::fs::read_to_string(&argv_file).expect("stub must have run");
    let args: Vec<&str> = argv.lines().collect();
    assert!(args.contains(&"-c"), "argv: {args:?}");
    assert!(args.contains(&"--all-errors"), "argv: {args:?}");
    let ypos = args.iter().position(|a| *a == "-Y").expect("-Y passed");
    assert!(!args[ypos + 1].is_empty(), "-Y needs a directory");
    let lpos = args.iter().position(|a| *a == "-L").expect("-L passed");
    assert_eq!(args[lpos + 1], "/opt/kamailio/modules");
    let fpos = args.iter().position(|a| *a == "-f").expect("-f passed");
    assert_eq!(args[fpos + 1], cfg.display().to_string());
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hanging_kamailio_check_is_bounded_and_reported() {
    let dir = std::env::temp_dir().join(format!("kamlsp-e2e-hang-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("kamailio-hang.sh");
    std::fs::write(&stub, "#!/bin/sh\nsleep 60\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, "request_route { exit; }\n").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env("KAMAILIO_LSP_BIN", stub.display().to_string())
        .env("KAMAILIO_LSP_CHECK_TIMEOUT_MS", "300")
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
    wait_for(&rx, |v| v["id"] == 1, "initialize result");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":format!("file://{}", cfg.display()),"languageId":"kamailio-cfg","version":1,"text":"request_route { exit; }\n"}}}),
    );

    // within a couple seconds (NOT 60) we must hear the timeout warning
    let log = wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("timed out")
        },
        "check-timeout logMessage",
    );
    assert!(
        log["params"]["message"]
            .as_str()
            .unwrap()
            .contains("timed out")
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_kamailio_bin_disables_checks_entirely() {
    let dir = std::env::temp_dir().join(format!("kamlsp-e2e-off-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, "request_route { exit; }\n").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env("KAMAILIO_LSP_BIN", "") // explicit opt-out
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
    wait_for(&rx, |v| v["id"] == 1, "initialize result");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":format!("file://{}", cfg.display()),"languageId":"kamailio-cfg","version":1,"text":"request_route { exit; }\n"}}}),
    );
    // request hover to force a full round-trip; NO publishDiagnostics
    // may arrive before its response
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{
        "textDocument":{"uri":format!("file://{}", cfg.display())},"position":{"line":0,"character":1}}}),
    );
    let mut saw_diags = false;
    loop {
        let v = wait_for(&rx, |_| true, "hover response");
        if v["method"] == "textDocument/publishDiagnostics" {
            saw_diags = true;
        }
        if v["id"] == 2 {
            break;
        }
    }
    assert!(!saw_diags, "diagnostics must be fully disabled");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn symbol_columns_are_utf16_on_multibyte_lines() {
    let dir = std::env::temp_dir().join(format!("kamlsp-utf16-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("u.cfg");
    // the comment holds é (2 bytes / 1 unit) and 😀 (4 bytes / 2 units):
    // byte col of `route` = 13, utf16 col = 10
    let text = "/* \u{e9}\u{1F600} */ route[x] { exit; }\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env("KAMAILIO_LSP_BIN", "")
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
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "symbols");
    let col = v["result"][0]["selectionRange"]["start"]["character"]
        .as_u64()
        .unwrap();
    assert_eq!(
        col, 10,
        "must be the UTF-16 column, not the byte column (13)"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn folding_and_nested_symbols_over_stdio() {
    let dir = std::env::temp_dir().join(format!("kamlsp-fold-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("fold.cfg");
    let text = "loadmodule \"tm.so\"\nrequest_route {\n    if (1) {\n        exit;\n    }\n}\nfailure_route[FR] {\n    xlog(\"}\");\n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env("KAMAILIO_LSP_BIN", "")
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
    assert!(
        init["result"]["capabilities"]["foldingRangeProvider"]
            .as_bool()
            .unwrap_or(false),
        "foldingRangeProvider must be advertised"
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
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/foldingRange","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "folding");
    let folds = v["result"].as_array().expect("folding array");
    assert_eq!(folds.len(), 2, "one fold per route block: {folds:?}");
    assert_eq!(folds[0]["startLine"], 1);
    assert_eq!(folds[0]["endLine"], 5);
    assert_eq!(folds[1]["startLine"], 6);
    assert_eq!(folds[1]["endLine"], 8);

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/documentSymbol","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "symbols");
    let syms = v["result"].as_array().expect("symbol array");
    assert_eq!(syms.len(), 2);
    // nested DocumentSymbol shape: full block range + selectionRange;
    // the unnamed request_route is named by its keyword
    assert_eq!(syms[0]["name"], "request_route");
    assert_eq!(syms[0]["range"]["start"]["line"], 1);
    assert_eq!(syms[0]["range"]["end"]["line"], 5);
    assert_eq!(syms[0]["range"]["end"]["character"], 1);
    assert_eq!(syms[0]["selectionRange"]["start"]["line"], 1);
    assert_eq!(syms[1]["name"], "failure_route[FR]");
    assert_eq!(syms[1]["range"]["end"]["line"], 8);
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}
