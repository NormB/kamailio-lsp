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
    let tree = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let wiki = common::required_env("KAMAILIO_LSP_TEST_WIKI");
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");

    let dir = std::env::temp_dir().join(format!("kamlsp-proof-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("proof.cfg");
    // an unknown modparam so the diagnostics leg has something to find
    let bad = format!("{CFG}modparam(\"tm\", \"no_such_param_xyz\", 1)\n");
    std::fs::write(&cfg, &bad).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", &bin) // empty = diagnostics off
            .env("KAMAILIO_LSP_SRC", &tree)
            .env("KAMAILIO_LSP_WIKI", &wiki)
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
    let _ = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
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

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", &bin)
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
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
    let mpath = common::required_env("KAMAILIO_LSP_TEST_MPATH");

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

/// An included FRAGMENT, opened on its own, against the REAL checker.
///
/// The stdio test for this uses a stub checker, and a stub can only
/// echo back what its author expected.  Two things only the real
/// parser decides: that a fragment handed to `-c` on its own is
/// rejected, and how it spells the path of a file reached through
/// `include_file`.
///
/// The fixture is built so the two possible behaviours cannot look
/// alike.  The fragment calls `t_relay()`, a command that exists only
/// because the ROOT loads `tm`; checked through the root the closure
/// is clean, checked on its own the parser says "unknown command,
/// missing loadmodule?".  A syntax error inside the fragment would
/// NOT discriminate — it reports at the same place either way — so
/// the presence of any `kamailio -c` diagnostic at all is the signal.
/// One genuinely undefined route keeps a publish coming so the
/// absence is observed rather than waited out.
#[test]
fn a_fragment_is_checked_through_its_root_by_the_real_parser() {
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
    let mpath = common::required_env("KAMAILIO_LSP_TEST_MPATH");

    let dir = std::env::temp_dir().join(format!("kamlsp-fragproof-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("inc")).unwrap();
    std::fs::write(
        dir.join("kamailio.cfg"),
        "#!KAMAILIO\nloadmodule \"sl.so\"\nloadmodule \"tm.so\"\ninclude_file \"inc/routes.cfg\"\nroute[PARENT_ONLY] {\n    exit;\n}\nrequest_route {\n    route(HANDLE);\n}\n",
    )
    .unwrap();
    let frag_text =
        "route[HANDLE] {\n    t_relay();\n    route(PARENT_ONLY);\n    route(NO_SUCH_ROUTE);\n}\n";
    let frag = dir.join("inc/routes.cfg");
    std::fs::write(&frag, frag_text).unwrap();
    let frag_uri = format!("file://{}", frag.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", &bin)
            .env(
                "KAMAILIO_LSP_CACHE_DIR",
                dir.join("cache").display().to_string(),
            )
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
            "initializationOptions":{"kamailioPath": bin, "modulesPath": mpath},
            "workspaceFolders":[{"uri": format!("file://{}", dir.display()), "name":"w"}]}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"kamailio/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "analysisRoot");
    assert!(
        v["result"]
            .as_str()
            .is_some_and(|s| s.ends_with("kamailio.cfg")),
        "the real workspace's root must be found: {v}"
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":frag_uri,"languageId":"kamailio-cfg","version":1,
                            "text":frag_text}}}),
    );
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["uri"] == frag_uri
                && !v["params"]["diagnostics"].as_array().unwrap().is_empty()
        },
        "diagnostics on the fragment",
    );
    let items = v["params"]["diagnostics"].as_array().unwrap().clone();
    let checker: Vec<&serde_json::Value> = items
        .iter()
        .filter(|d| d["source"] == "kamailio -c")
        .collect();
    assert!(
        checker.is_empty(),
        "the closure compiles; any checker error here means the FRAGMENT \
         was handed to `-c` instead of its root: {checker:?}"
    );
    // the route the ROOT defines is in scope...
    assert!(
        !items.iter().any(|d| d["message"]
            .as_str()
            .is_some_and(|m| m.contains("PARENT_ONLY"))),
        "a route the root defines must not read as undefined: {items:?}"
    );
    // ...and a route nothing defines is still reported, so the absence
    // above is a decision and not a silenced analyzer
    assert!(
        items.iter().any(|d| d["message"]
            .as_str()
            .is_some_and(|m| m.contains("NO_SUCH_ROUTE"))),
        "a genuinely undefined route must still be flagged: {items:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Oracle: the REAL parser must see the same configuration whether it
/// is one file or many.
///
/// Everything else here compares this server against itself.  This
/// compares the layout against the thing that decides — `kamailio -c`
/// on a whole config, and on the same text split across
/// `include_file`s.  If the parser's own findings move when a
/// configuration is split, then "a fragment is a layout, not a
/// different program" is false and this whole feature rests on a
/// wrong premise.
#[test]
fn the_real_parser_sees_the_same_program_whole_or_split() {
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
    let mpath = common::required_env("KAMAILIO_LSP_TEST_MPATH");

    let dir = std::env::temp_dir().join(format!("kamlsp-oracle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("inc")).unwrap();

    // three blocks, the middle one carrying a real syntax error
    let blocks = [
        "route[ONE] {\n    exit;\n}\n",
        "route[TWO] {\n    exit\n}\n",
        "route[THREE] {\n    exit;\n}\n",
    ];
    let head = "#!KAMAILIO\n";

    let run = |cfg: &std::path::Path| -> Vec<String> {
        let mut cmd = Command::new(&bin);
        cmd.arg("-c")
            .arg("--all-errors")
            .arg("-Y")
            .arg(std::env::temp_dir());
        if !mpath.is_empty() {
            cmd.arg("-L").arg(&mpath);
        }
        let out = cmd
            .arg("-f")
            .arg(cfg)
            .current_dir(cfg.parent().unwrap())
            .output()
            .expect("the checker runs");
        let stderr = String::from_utf8_lossy(&out.stderr);
        kamailio_lsp::diag::parse_check_output(&stderr, out.status.code().unwrap_or(0))
            .into_iter()
            .filter(|d| !d.file.is_empty())
            // the message and the column, NOT the file or the
            // absolute line: those move by construction
            .map(|d| format!("{}:{}", d.col_start, d.message))
            .collect()
    };

    let whole = dir.join("whole.cfg");
    std::fs::write(&whole, format!("{head}{}", blocks.concat())).unwrap();
    let from_whole = run(&whole);
    assert!(
        !from_whole.is_empty(),
        "the fixture must make the real parser complain, or this proves nothing"
    );

    let root = dir.join("kamailio.cfg");
    let mut root_text = String::from(head);
    for (i, b) in blocks.iter().enumerate() {
        root_text.push_str(&format!("include_file \"inc/b{i}.cfg\"\n"));
        std::fs::write(dir.join(format!("inc/b{i}.cfg")), b).unwrap();
    }
    std::fs::write(&root, &root_text).unwrap();
    let from_split = run(&root);

    assert_eq!(
        from_whole, from_split,
        "the real parser reports something different once the same text \
         is split across includes — the premise this feature rests on"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
