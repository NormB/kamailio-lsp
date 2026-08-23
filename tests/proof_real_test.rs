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

#[test]
fn real_binary_include_errors_are_never_silent() {
    let Ok(_) = std::env::var("KAMAILIO_LSP_TEST_TREE") else {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_TREE not set");
        return;
    };
    let bin = std::env::var("KAMAILIO_LSP_TEST_BIN").unwrap_or_default();
    if bin.is_empty() {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_BIN not set");
        return;
    }
    let dir = std::env::temp_dir().join(format!("kamlsp-proof-inc-{}", std::process::id()));
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .env("KAMAILIO_LSP_BIN", &bin)
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
        "the REAL binary's include error must surface: {d}"
    );
    let msg = ds[0]["message"].as_str().unwrap();
    assert!(
        msg.contains("sub_bad.cfg"),
        "diagnostic names the broken include: {msg}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

/// The formatter's strongest claim, checked against the real parser:
/// a config the parser accepts before formatting is still accepted
/// after, under every indent style a client can ask for.
///
/// The criterion is the POSITIONED parse errors, not the exit status.
/// `-c` also runs checks that have nothing to do with the config text,
/// and those surface as the unpositioned fallback diagnostic (empty
/// `file`); keying the proof on the exit code would measure those
/// instead of the parse.
#[test]
fn formatting_never_changes_what_the_real_parser_accepts() {
    let Ok(bin) = std::env::var("KAMAILIO_LSP_TEST_BIN") else {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_BIN not set");
        return;
    };
    let mpath = std::env::var("KAMAILIO_LSP_TEST_MPATH").unwrap_or_default();

    // ragged on purpose, and carrying every trap the formatter has to
    // respect: braces in a string and in both comment styles, a block
    // comment with its own alignment, a continued directive, nesting
    let src = "#!KAMAILIO\n\
        #!define LONG one \\\n\
            two\n\
        loadmodule \"sl.so\"\n\
        loadmodule \"tm.so\"\n\
        loadmodule \"pv.so\"\n\
        modparam(\"tm\", \"fr_timer\", 30000)\n\
        request_route {\n\
        $var(s) = \"a { brace } in a string\";\n\
        # a } in a comment\n\
        // another } comment\n\
        /* block { comment\n\
           aligned } body */\n\
        if ($rU == \"x\") {\n\
        t_relay();\n\
        } else {\n\
        exit;\n\
        }\n\
        }\n";

    let dir = std::env::temp_dir().join(format!("kamlsp-fmtproof-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let parse_errors = |text: &str, tag: &str| -> Vec<String> {
        let cfg = dir.join(format!("{tag}.cfg"));
        std::fs::write(&cfg, text).unwrap();
        let mut cmd = Command::new(&bin);
        cmd.arg("-c").arg("--all-errors").arg("-Y").arg(&dir);
        if !mpath.is_empty() {
            cmd.arg("-L").arg(&mpath);
        }
        let out = cmd.arg("-f").arg(&cfg).output().expect("the checker runs");
        let stderr = String::from_utf8_lossy(&out.stderr);
        kamailio_lsp::diag::parse_check_output(&stderr, out.status.code().unwrap_or(0))
            .into_iter()
            .filter(|d| !d.file.is_empty())
            .map(|d| format!("{}:{}:{}", d.line, d.col_start, d.message))
            .collect()
    };

    let baseline = parse_errors(src, "before");
    assert!(
        baseline.is_empty(),
        "the fixture must parse cleanly or the proof is vacuous: {baseline:?}"
    );

    for (label, insert_spaces, tab_size) in [("tabs", false, 4), ("2sp", true, 2), ("8sp", true, 8)]
    {
        let opts = kamailio_lsp::format::Options {
            insert_spaces,
            tab_size,
        };
        let out = kamailio_lsp::format::format(src, &opts);
        assert_eq!(
            parse_errors(&out, label),
            baseline,
            "formatting with {label} changed what the real parser reports:\n{out}"
        );
        assert_eq!(
            out,
            kamailio_lsp::format::format(&out, &opts),
            "formatting was not idempotent under {label}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
