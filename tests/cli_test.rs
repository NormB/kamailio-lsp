//! The `kamailio-lsp check` CLI mode: the same analyzer (plus the
//! real `kamailio -c` when a binary is configured) as a CI/git-hook
//! command.

mod common;

use std::process::Command;

fn setup(tag: &str) -> common::Scratch {
    let dir = common::Scratch::new(&format!("kamlsp-cli-{tag}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn check_reports_analyzer_warnings_and_exits_zero() {
    let dir = setup("warn");
    let cfg = dir.join("w.cfg");
    std::fs::write(&cfg, "request_route {\n    route(MISSING);\n}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", cfg.to_str().unwrap()])
        .env("KAMAILIO_LSP_BIN", "")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // 1-based positions for humans, grep-able shape
    assert!(
        stdout.contains("w.cfg:2:11: warning: route 'MISSING' is not defined"),
        "got: {stdout}"
    );
    assert_eq!(out.status.code(), Some(0), "warnings alone exit 0");
    // --strict promotes warnings to failure
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", "--strict", cfg.to_str().unwrap()])
        .env("KAMAILIO_LSP_BIN", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "--strict fails on warnings");
}

#[test]
fn check_runs_the_configured_binary_and_fails_on_errors() {
    let dir = setup("bin");
    let cfg = dir.join("b.cfg");
    std::fs::write(&cfg, "request_route {\n    exit;\n}\n").unwrap();
    // stub checker records argv and emits one positioned kamailio-shape error
    let argv_file = dir.join("argv");
    let stub = dir.join("stub.sh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nfor a in \"$@\"; do cfg=\"$a\"; done\necho \" 0(1) CRITICAL: <core> [core/cfg.y:4048]: yyerror_at(): parse error in config file $cfg, line 2, column 5-9: planted error\" >&2\nexit 255\n",
            argv_file.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut p = std::fs::metadata(&stub).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&stub, p).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args([
            "check",
            "--bin",
            stub.to_str().unwrap(),
            cfg.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("b.cfg:2:5: error: planted error"),
        "got: {stdout}"
    );
    assert_eq!(out.status.code(), Some(1), "errors exit 1");
    // the invocation carries the kamailio check flags
    let argv = std::fs::read_to_string(&argv_file).unwrap();
    let args: Vec<&str> = argv.lines().collect();
    assert!(args.contains(&"-c"), "argv: {args:?}");
    assert!(args.contains(&"--all-errors"), "argv: {args:?}");
    assert!(args.contains(&"-Y"), "argv: {args:?}");
}

#[test]
fn check_remaps_include_errors_onto_the_root() {
    let dir = setup("inc");
    std::fs::create_dir_all(dir.join("incdir")).unwrap();
    std::fs::write(dir.join("incdir/sub.cfg"), "route[SUB] {\n    exit;\n}\n").unwrap();
    let cfg = dir.join("main.cfg");
    std::fs::write(
        &cfg,
        "#!KAMAILIO\ninclude_file \"incdir/sub.cfg\"\nrequest_route { exit; }\n",
    )
    .unwrap();
    // stub echoes the include path AS WRITTEN (the real 6.0.1 and
    // 6.1.5 shape)
    let stub = dir.join("stub.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\necho \" 0(1) CRITICAL: <core> [core/cfg.y:4048]: yyerror_at(): parse error in config file incdir/sub.cfg, line 2, column 5: syntax error\" >&2\nexit 255\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut p = std::fs::metadata(&stub).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&stub, p).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args([
            "check",
            "--bin",
            stub.to_str().unwrap(),
            cfg.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout
            .contains("main.cfg:2:1: error: in included file incdir/sub.cfg, line 2: syntax error"),
        "foreign diags must attach to the include directive: {stdout}"
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn check_clean_file_exits_zero_and_bad_usage_exits_two() {
    let dir = setup("clean");
    let cfg = dir.join("c.cfg");
    std::fs::write(
        &cfg,
        "route[A] {\n    exit;\n}\nrequest_route {\n    route(A);\n}\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", cfg.to_str().unwrap()])
        .env("KAMAILIO_LSP_BIN", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    // missing file → usage-style failure, not a panic
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", "/nonexistent/x.cfg"])
        .env("KAMAILIO_LSP_BIN", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "unreadable file exits 2");
    // no files at all
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    // unknown flag
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", "--nope", "x.cfg"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn check_against_the_real_binary_when_gated() {
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
    let dir = setup("real");
    let bad = dir.join("bad.cfg");
    std::fs::write(
        &bad,
        "#!KAMAILIO\nloadmodule \"tm.so\"\nmodparam(\"tm\", \"fr_tmer\", 30000)\nrequest_route { exit; }\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", "--bin", &bin, bad.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("error: Can't set module parameter"),
        "the real parser's verdict shows: {stdout}"
    );
    assert_eq!(out.status.code(), Some(1));
}

/// The CLI must know about fragments too.
///
/// `check` is what a git hook and a CI job run, and the analyzer it
/// runs is the same one the server runs — but it had no idea a file
/// might be part of something larger.  Hand it a correct split
/// configuration and it reported the parent's routes as undefined;
/// with `--strict` those warnings are errors, so a green
/// configuration failed the build.
#[test]
fn check_understands_a_split_configuration() {
    let dir = setup("split");
    std::fs::create_dir_all(dir.join("inc")).unwrap();
    let root = dir.join("kamailio.cfg");
    std::fs::write(
        &root,
        "#!KAMAILIO
include_file \"inc/routes.cfg\"\nroute[HELPER] {\n    exit;\n}\nrequest_route {\n    route(ENTRY);\n}\n",
    )
    .unwrap();
    let frag = dir.join("inc/routes.cfg");
    std::fs::write(&frag, "route[ENTRY] {\n    route(HELPER);\n}\n").unwrap();

    // the whole configuration, as a hook would pass it
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", "--strict", "--bin", ""])
        .arg(&root)
        .arg(&frag)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        !stdout.contains("HELPER"),
        "the fragment's parent defines HELPER; reporting it undefined fails a \
         correct configuration:\n{stdout}"
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "--strict must not fail a correct split configuration:\n{stdout}"
    );

    // and the fragment on its own, which is what a hook passes when
    // only that file changed
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", "--strict", "--bin", ""])
        .arg(&frag)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        !stdout.contains("HELPER"),
        "the root is right there beside it:\n{stdout}"
    );
    assert_eq!(out.status.code(), Some(0), "{stdout}");

    // a route defined nowhere is still reported, so the silence above
    // is analysis and not a disabled analyzer
    std::fs::write(
        &frag,
        "route[ENTRY] {\n    route(HELPER);\n    route(NOWHERE);\n}\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", "--bin", ""])
        .arg(&frag)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(stdout.contains("NOWHERE"), "{stdout}");
}

/// The CLI runs the same `modparam` catalogue check the editor does.
///
/// It did not, so a configuration was green in CI and warned about in
/// the editor — the two surfaces disagreeing about the same file.
#[test]
fn check_runs_the_catalogue_check_and_names_the_catalogue() {
    let dir = setup("catalogue");
    let cfg = dir.join("m.cfg");
    // a real tm parameter and one no Kamailio release exports
    std::fs::write(
        &cfg,
        "request_route {\n    xlog(\"x\");\n}\nmodparam(\"tm\", \"fr_timer\", 5)\nmodparam(\"tm\", \"not_a_real_parameter\", 1)\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", cfg.to_str().unwrap()])
        .env("KAMAILIO_LSP_BIN", "")
        .env("KAMAILIO_LSP_SRC", "")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        stdout.contains("not_a_real_parameter"),
        "the catalogue check must run in the CLI; stdout: {stdout}"
    );
    // the version belongs in the message, so a pasted line explains
    // itself without the header
    assert!(
        stdout.contains("is not exported by module 'tm' in Kamailio"),
        "the finding must name the catalogue; stdout: {stdout}"
    );
    assert!(
        !stdout.contains("fr_timer"),
        "a real parameter must not warn; stdout: {stdout}"
    );
    // findings are `file:line:col: sev: msg` for other tools to parse,
    // so the header goes to stderr rather than among them
    assert!(
        stderr.contains("modparam checks against Kamailio"),
        "the run must say what it judged against; stderr: {stderr}"
    );
}

/// The release checked against is selectable, and selecting one the
/// catalogue really covers changes the answer.
///
/// `rr::ignore_user` exists in 5.8.8 and 6.0.8 and was dropped by
/// 6.1: on the newest release it warns and says where it exists, and
/// on 6.0.8 it is simply correct. A test that only checked the header
/// would pass even if the selection changed nothing.
#[test]
fn the_checked_release_is_selectable() {
    let dir = setup("version");
    let cfg = dir.join("v.cfg");
    std::fs::write(
        &cfg,
        "request_route {\n    xlog(\"x\");\n}\nmodparam(\"rr\", \"ignore_user\", 1)\n",
    )
    .unwrap();
    let run = |version: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .args(["check", cfg.to_str().unwrap()])
            .env("KAMAILIO_LSP_BIN", "")
            .env("KAMAILIO_LSP_SRC", "")
            .env("KAMAILIO_LSP_VERSION", version)
            .output()
            .unwrap();
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };

    // the newest release: warns, and says which releases do have it
    let (newest_out, newest_err) = run("");
    assert!(newest_out.contains("ignore_user"), "stdout: {newest_out}");
    assert!(
        newest_out.contains("it exists in"),
        "a parameter another release exports must say so; stdout: {newest_out}"
    );
    assert!(newest_err.contains("6.1.5"), "stderr: {newest_err}");

    // a release that really exports it: silent
    let (older_out, older_err) = run("6.0.8");
    assert!(older_err.contains("6.0.8"), "stderr: {older_err}");
    assert!(
        !older_out.contains("ignore_user"),
        "6.0.8 exports it, so there is nothing to warn about; stdout: {older_out}"
    );
}

/// An unsupported release must not quietly become the newest — the
/// run would then report on a release nobody asked about.
#[test]
fn an_unsupported_release_is_reported_not_swallowed() {
    let dir = setup("badversion");
    let cfg = dir.join("b.cfg");
    std::fs::write(&cfg, "request_route {\n    xlog(\"x\");\n}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", cfg.to_str().unwrap()])
        .env("KAMAILIO_LSP_BIN", "")
        .env("KAMAILIO_LSP_SRC", "")
        .env("KAMAILIO_LSP_VERSION", "9.9.9")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no built-in catalogue for Kamailio 9.9.9"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("supported:"), "stderr: {stderr}");
    assert!(stderr.contains("using"), "stderr: {stderr}");
}

/// The checker runs in the config's directory, so relative includes
/// resolve as they do for the real server, and is handed the config
/// by a path that is valid from there.  A bare `fn.cfg` used to make
/// the working directory the empty path (the spawn failed), and
/// `sub/fn.cfg` was handed over unchanged while the checker already
/// ran inside `sub/`, so it could not open it.
#[test]
fn check_accepts_relative_paths() {
    let dir = setup("relative");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("fn.cfg"), "request_route {\n    exit;\n}\n").unwrap();
    std::fs::write(dir.join("sub/fn.cfg"), "request_route {\n    exit;\n}\n").unwrap();
    // stub checker: fails unless the last argument opens from its cwd
    let stub = dir.join("stub.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\nfor a in \"$@\"; do cfg=\"$a\"; done\n[ -f \"$cfg\" ] && exit 0\necho \"cannot open $cfg from $(pwd)\" >&2\nexit 255\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut p = std::fs::metadata(&stub).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&stub, p).unwrap();
    for rel in ["fn.cfg", "sub/fn.cfg", "./sub/fn.cfg"] {
        let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .current_dir(&dir)
            .args(["check", "--bin", stub.to_str().unwrap(), rel])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{rel}: stdout {} stderr {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// A missing include is reported on the directive that names it, not
/// at line 1 column 1 of the file being checked.
#[test]
fn a_missing_include_is_reported_on_its_directive() {
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
    let dir = setup("missing-inc");
    let cfg = dir.join("m.cfg");
    std::fs::write(
        &cfg,
        "request_route {\n    exit;\n}\n  include_file \"nope.cfg\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", "--bin", &bin, cfg.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("m.cfg:4:3: error: cannot open included file: nope.cfg"),
        "{stdout}"
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn check_reads_a_config_that_is_not_utf8() {
    // one latin-1 byte in a comment is not a reason to refuse the file:
    // the editor and the include graph read it, so `check` must too
    let dir = setup("latin1");
    let cfg = dir.join("l.cfg");
    std::fs::write(
        &cfg,
        b"# caf\xe9\nrequest_route {\n    route(MISSING);\n}\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", cfg.to_str().unwrap()])
        .env("KAMAILIO_LSP_BIN", "")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("l.cfg:3:11: warning: route 'MISSING' is not defined"),
        "stdout: {stdout} stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
}

/// `--modules-path` is the CLI's `modulesPath`: the checker gets it as
/// `-L`.  An empty directory makes `loadmodule "tm.so"` fail in the
/// real parser (`could not find module <tm> in <dir>`), and the
/// module directory of the build makes it load.
#[test]
fn the_modules_path_reaches_the_checker() {
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
    let mpath = common::required_env("KAMAILIO_LSP_TEST_MPATH");
    let dir = setup("mpath");
    std::fs::create_dir_all(dir.join("empty")).unwrap();
    let cfg = dir.join("m.cfg");
    std::fs::write(
        &cfg,
        "loadmodule \"tm.so\"\nrequest_route {\n    exit;\n}\n",
    )
    .unwrap();
    let run = |mp: &str| {
        Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .args([
                "check",
                "--bin",
                &bin,
                "--modules-path",
                mp,
                cfg.to_str().unwrap(),
            ])
            .output()
            .unwrap()
    };
    let empty = run(dir.join("empty").to_str().unwrap());
    let stdout = String::from_utf8_lossy(&empty.stdout);
    assert!(
        stdout.contains("m.cfg:1:12: error: failed to load module"),
        "{stdout}"
    );
    assert_eq!(empty.status.code(), Some(1));
    let real = run(&mpath);
    assert_eq!(
        real.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&real.stdout)
    );
    // a relative path means the caller's working directory, not the
    // config's directory the checker runs in
    std::fs::create_dir_all(dir.join("conf")).unwrap();
    let nested = dir.join("conf/n.cfg");
    std::fs::copy(&cfg, &nested).unwrap();
    std::os::unix::fs::symlink(&mpath, dir.join("mods")).unwrap();
    let rel = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .current_dir(&dir)
        .args([
            "check",
            "--bin",
            &bin,
            "--modules-path",
            "mods",
            "conf/n.cfg",
        ])
        .output()
        .unwrap();
    assert_eq!(
        rel.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&rel.stdout)
    );
    // a flag without its value is a usage error
    let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
        .args(["check", "--modules-path"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

/// `check` warns when a configuration's include closure stops at the
/// 500-file bound, on the checked file's first line; at 500 files in
/// all, nothing is left out and nothing is said.
#[test]
fn check_says_when_the_include_closure_stops_at_its_bound() {
    for (includes, warned) in [(500usize, true), (499, false)] {
        let dir = setup(&format!("bound-{includes}"));
        std::fs::create_dir_all(dir.join("inc")).unwrap();
        let mut root = String::new();
        for i in 0..includes {
            root.push_str(&format!("include_file \"inc/c{i}.cfg\"\n"));
            std::fs::write(
                dir.join(format!("inc/c{i}.cfg")),
                "route[x] {\n    exit;\n}\n",
            )
            .unwrap();
        }
        let cfg = dir.join("kamailio.cfg");
        std::fs::write(&cfg, &root).unwrap();
        let out = Command::new(env!("CARGO_BIN_EXE_kamailio-lsp"))
            .env("KAMAILIO_LSP_BIN", "")
            .args(["check", "--bin", ""])
            .arg(&cfg)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let said =
            stdout.contains("kamailio.cfg:1:1: warning: the include closure stopped at 500 files");
        assert_eq!(said, warned, "{includes} includes:\n{stdout}");
    }
}
