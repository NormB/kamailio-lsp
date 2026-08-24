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
    // range error and one spanning error, in the real 6.0.1/6.1.4 shapes
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

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", stub.display().to_string())
            .env("KAMAILIO_LSP_SRC", "") // no catalog needed for this flow
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("server binary must spawn"),
        &dir,
    );
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
    // real clients close the pipe after `exit`; tower-lsp-server's serve loop
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

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env_remove("KAMAILIO_LSP_BIN")
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

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", stub.display().to_string())
            .env("KAMAILIO_LSP_CHECK_TIMEOUT_MS", "300")
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

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", "") // explicit opt-out
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

#[test]
fn references_rename_highlight_over_stdio() {
    let dir = std::env::temp_dir().join(format!("kamlsp-refs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("refs.cfg");
    let text = "request_route {\n    route(RELAY);\n    route(\"RELAY\");\n}\nroute[RELAY] {\n    exit;\n}\n";
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
    let init = wait_for(&rx, |v| v["id"] == 1, "init");
    for cap in ["referencesProvider", "documentHighlightProvider"] {
        assert!(
            init["result"]["capabilities"][cap]
                .as_bool()
                .unwrap_or(false),
            "{cap} must be advertised"
        );
    }
    assert!(
        !init["result"]["capabilities"]["renameProvider"].is_null(),
        "renameProvider must be advertised"
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
    // references from a call site, declaration included → 2 refs + def
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},
        "context":{"includeDeclaration":true}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "references");
    let locs = v["result"].as_array().expect("locations");
    assert_eq!(locs.len(), 3, "2 refs + 1 def: {locs:?}");
    // declaration excluded → 2
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/references","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},
        "context":{"includeDeclaration":false}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "references-nodecl");
    assert_eq!(v["result"].as_array().unwrap().len(), 2);
    // highlights at the def name: 3 occurrences, def is Write(3)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"textDocument/documentHighlight","params":{
        "textDocument":{"uri":uri},"position":{"line":4,"character":7}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 4, "highlight");
    let hls = v["result"].as_array().expect("highlights");
    assert_eq!(hls.len(), 3);
    assert!(
        hls.iter().any(|h| h["kind"] == 3),
        "def highlighted as Write"
    );
    // rename to a valid unquoted ID
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":5,"method":"textDocument/rename","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},"newName":"fwd_1"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 5, "rename");
    let edits = v["result"]["changes"][&uri].as_array().expect("edits");
    assert_eq!(edits.len(), 3);
    assert!(edits.iter().all(|e| e["newText"] == "fwd_1"));
    // the quoted ref's edit replaces only the name inside the quotes
    let quoted = edits
        .iter()
        .find(|e| e["range"]["start"]["line"] == 2)
        .unwrap();
    assert_eq!(quoted["range"]["start"]["character"], 11);
    assert_eq!(quoted["range"]["end"]["character"], 16);
    // rename to an ILLEGAL name is rejected with an error — including
    // names the parser only accepts QUOTED (colon/dot/dash/leading
    // digit): definitions are commonly unquoted, so such a rename
    // would emit a config kamailio rejects
    for (id, bad) in [(6, "has space"), (7, "fwd:1"), (8, "a.b"), (9, "1ab")] {
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/rename","params":{
            "textDocument":{"uri":uri},"position":{"line":1,"character":11},"newName":bad}}),
        );
        let v = wait_for(&rx, |v| v["id"] == id, "rename-bad");
        assert!(
            !v["error"].is_null(),
            "illegal new name '{bad}' must be a jsonrpc error: {v}"
        );
    }
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn event_route_names_cannot_be_renamed_but_still_highlight() {
    let dir = std::env::temp_dir().join(format!("kamlsp-evr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("ev.cfg");
    let text = "event_route[htable:mod-init] {\n    exit;\n}\n";
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
    // highlights on the colon name keep working
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentHighlight","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":15}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "highlight");
    assert_eq!(v["result"].as_array().unwrap().len(), 1);
    // rename on the event-route name is rejected: the name is the
    // module's event identifier, not a script symbol
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/rename","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":15},"newName":"valid_id"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "rename-event");
    assert!(
        !v["error"].is_null(),
        "event-route rename must be rejected: {v}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn include_file_errors_surface_on_the_include_directive() {
    // kamailio attributes errors in included files to the include path
    // AS WRITTEN (often relative to its own cwd); such diagnostics
    // must attach to the include_file directive, never vanish
    let dir = std::env::temp_dir().join(format!("kamlsp-incdiag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("incdir")).unwrap();
    std::fs::write(
        dir.join("incdir/sub_bad.cfg"),
        "route[SUB] {\n    bogus_stuff;\n}\n",
    )
    .unwrap();
    let text = "#!KAMAILIO\ninclude_file \"incdir/sub_bad.cfg\"\nrequest_route { exit; }\n";
    let cfg = dir.join("main.cfg");
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    // stub echoing the REAL capture shape: relative include path
    let stub = dir.join("stub.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\necho \" 0(1) CRITICAL: <core> [core/cfg.y:4048]: yyerror_at(): parse error in config file incdir/sub_bad.cfg, line 2, column 16: syntax error\" >&2\nexit 255\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", stub.display().to_string())
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
    let d = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publish",
    );
    let ds = d["params"]["diagnostics"].as_array().unwrap();
    assert!(
        !ds.is_empty(),
        "a broken include must NOT produce zero diagnostics: {d}"
    );
    assert_eq!(
        ds[0]["range"]["start"]["line"], 1,
        "attached at the include_file directive: {ds:?}"
    );
    let msg = ds[0]["message"].as_str().unwrap();
    assert!(
        msg.contains("incdir/sub_bad.cfg") && msg.contains("syntax error"),
        "message carries the include context: {msg}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn filtered_out_diags_still_yield_a_root_fallback() {
    // rc!=0 with positioned errors pointing at a file OUTSIDE the
    // include closure (e.g. a nested include): everything would be
    // filtered — a fallback diagnostic must still surface
    let dir = std::env::temp_dir().join(format!("kamlsp-fallb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("main.cfg");
    let text = "#!KAMAILIO\nrequest_route { exit; }\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    let stub = dir.join("stub.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\necho \" 0(1) CRITICAL: <core> [core/cfg.y:4048]: yyerror_at(): parse error in config file deep/nested.cfg, line 7, column 2: syntax error\" >&2\nexit 255\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", stub.display().to_string())
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
    let d = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publish",
    );
    let ds = d["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(ds.len(), 1, "one fallback diagnostic: {d}");
    assert_eq!(ds[0]["range"]["start"]["line"], 0);
    let msg = ds[0]["message"].as_str().unwrap();
    assert!(
        msg.contains("deep/nested.cfg") && msg.contains("syntax error"),
        "fallback carries the dropped context: {msg}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checker_runs_in_the_configs_directory() {
    // cwd parity with the CLI: the -c subprocess must run with its
    // cwd set to the config's own directory, so relative
    // include_file paths resolve identically in editor and CI runs.
    // The stub fails unless ./inc.cfg is visible from its cwd.
    let dir = std::env::temp_dir().join(format!("kamlsp-e2e-cwd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("inc.cfg"), "route[H] {\n    exit;\n}\n").unwrap();
    let stub = dir.join("kamailio-stub.sh");
    std::fs::write(
        &stub,
        format!(
            "{STUB_PREAMBLE}if [ -f inc.cfg ]; then exit 0; fi\necho \" 0(1) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file $cfg, line 1, column 1-13: failed to open included file inc.cfg\" >&2\nexit 255\n"
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();
    let text = "include_file \"inc.cfg\"\nrequest_route { exit; }\n";
    let cfg = dir.join("main.cfg");
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", stub.display().to_string())
            .env("KAMAILIO_LSP_SRC", "")
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
    let d = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publish",
    );
    let ds = d["params"]["diagnostics"].as_array().unwrap();
    assert!(
        ds.is_empty(),
        "relative include must resolve from the config's directory: {ds:?}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn newer_save_supersedes_a_slow_check() {
    // latest-wins: a slow -c run on stale content must neither delay
    // nor outlive a newer save of the same document — the newer
    // check's diagnostics arrive promptly (well under the stale
    // run's 60s sleep; the harness bounds the wait at 15s).
    let dir = std::env::temp_dir().join(format!("kamlsp-e2e-super-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("kamailio-stub.sh");
    std::fs::write(
        &stub,
        format!(
            "{STUB_PREAMBLE}if grep -q SLOW_MARKER \"$cfg\"; then\n  sleep 60\n  echo \" 0(1) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file $cfg, line 1, column 1-2: slow stale error\" >&2\n  exit 255\nfi\necho \" 0(1) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file $cfg, line 1, column 1-2: fresh error\" >&2\nexit 255\n"
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();
    let slow_text = "# SLOW_MARKER\nrequest_route { exit; }\n";
    let fast_text = "request_route { exit; }\n";
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, slow_text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", stub.display().to_string())
            .env("KAMAILIO_LSP_SRC", "")
            // the run timeout must not rescue the old serialized behavior:
            // only superseding the slow run can produce a timely result
            .env("KAMAILIO_LSP_CHECK_TIMEOUT_MS", "60000")
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
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    // v1 opens with the slow content: its check starts and hangs
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"kamailio-cfg","version":1,"text":slow_text}}}),
    );
    // give the slow check a moment to actually be in flight (ordering
    // matters, exact timing does not)
    std::thread::sleep(Duration::from_millis(300));
    // v2 saves fast content: it must supersede the slow run
    std::fs::write(&cfg, fast_text).unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":fast_text}]}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
        "textDocument":{"uri":uri}}}),
    );
    let d = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty())
        },
        "the newer save's diagnostics",
    );
    let ds = d["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(
        ds[0]["message"], "fresh error",
        "the newer content's result must win: {d}"
    );
    assert_eq!(d["params"]["version"], 2, "results belong to v2: {d}");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}
