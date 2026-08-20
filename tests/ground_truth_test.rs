//! Differential gates that keep the language model honest against the
//! real binary — the structural fix for model-vs-reality drift.
//!
//! Gated like the corpus sweep: KAMAILIO_LSP_TEST_TREE points at a
//! Kamailio source tree, KAMAILIO_LSP_TEST_BIN at a kamailio binary.

use kamailio_lsp::logic;

fn corpus(tree: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut push_dir = |d: std::path::PathBuf, recurse: bool| {
        let mut stack = vec![d];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() && recurse {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "cfg") {
                    out.push(p);
                }
            }
        }
    };
    push_dir(tree.join("etc"), false);
    push_dir(tree.join("misc").join("examples"), true);
    if let Ok(rd) = std::fs::read_dir(tree.join("src").join("modules")) {
        for e in rd.flatten() {
            push_dir(e.path().join("examples"), false);
        }
    }
    out.sort();
    out
}

fn accepts(bin: &str, f: &std::path::Path) -> bool {
    std::process::Command::new(bin)
        .args(["-c", "--all-errors", "-Y"])
        .arg(std::env::temp_dir())
        .arg("-f")
        .arg(f)
        .current_dir(f.parent().unwrap())
        .output()
        .map(|o| o.status.code() == Some(0))
        .unwrap_or(false)
}

/// The same shape of loader the server uses: plain disk, size-capped.
fn disk_loader(p: &std::path::Path) -> Option<String> {
    let md = std::fs::metadata(p).ok()?;
    if !md.is_file() || md.len() > 1_048_576 {
        return None;
    }
    std::fs::read_to_string(p).ok()
}

/// G1: on every corpus config the REAL parser accepts, the analyzer
/// must stay silent.  Any analyzer false positive on a real accepted
/// config is a CI failure forever.
#[test]
fn analyzer_is_silent_on_every_accepted_corpus_config() {
    let Ok(tree) = std::env::var("KAMAILIO_LSP_TEST_TREE") else {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_TREE not set");
        return;
    };
    let Ok(bin) = std::env::var("KAMAILIO_LSP_TEST_BIN") else {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_BIN not set");
        return;
    };
    let files = corpus(std::path::Path::new(&tree));
    assert!(files.len() >= 20, "expected a real corpus");
    let mut accepted = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for f in &files {
        if !accepts(&bin, f) {
            continue;
        }
        accepted += 1;
        let text = String::from_utf8_lossy(&std::fs::read(f).unwrap()).into_owned();
        let ds = logic::analyzer_diagnostics(f, &text, &disk_loader);
        for d in ds {
            failures.push(format!(
                "{}:{}: {} (analyzer warns on a config the binary accepts)",
                f.display(),
                d.line + 1,
                d.message
            ));
        }
    }
    assert!(
        accepted >= 3,
        "suspiciously few accepted configs: {accepted}"
    );
    assert!(
        failures.is_empty(),
        "analyzer false positives on {accepted} accepted configs:\n{}",
        failures.join("\n")
    );
    eprintln!("analyzer silent on all {accepted} accepted corpus configs");
}

/// G2: renaming through the real logic path can never emit a config
/// the parser rejects — proven mechanically with the binary.
#[test]
fn rename_round_trips_through_the_real_parser() {
    let Ok(bin) = std::env::var("KAMAILIO_LSP_TEST_BIN") else {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_BIN not set");
        return;
    };
    let dir = std::env::temp_dir().join(format!("kamlsp-roundtrip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // a named route with bare AND quoted call sites
    let original = "#!KAMAILIO\nroute[OLD_NAME] {\n    exit;\n}\nrequest_route {\n    route(OLD_NAME);\n    route(\"OLD_NAME\");\n}\n";
    let before = dir.join("before.cfg");
    std::fs::write(&before, original).unwrap();
    if !accepts(&bin, &before) {
        // a different binary generation rejecting the fixture must not
        // fail the gate — it only proves nothing
        eprintln!("SKIP: the test binary rejects the baseline fixture");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    // rename OLD_NAME -> fwd_2 through the shipped occurrence logic
    let new_name = "fwd_2";
    assert!(logic::valid_route_name(new_name));
    let mut occ = logic::route_occurrences(original, "OLD_NAME");
    assert_eq!(occ.len(), 3, "def + 2 call sites: {occ:?}");
    // apply edits back-to-front so earlier spans stay valid
    occ.sort_by_key(|(l, _)| (std::cmp::Reverse(l.line), std::cmp::Reverse(l.col)));
    let mut lines: Vec<String> = original.lines().map(str::to_string).collect();
    for (l, _) in &occ {
        let line = &mut lines[l.line as usize];
        let s = l.col as usize;
        let e = s + l.name.len();
        line.replace_range(s..e, new_name);
    }
    let renamed = format!("{}\n", lines.join("\n"));
    assert!(!renamed.contains("OLD_NAME"), "{renamed}");

    let after = dir.join("after.cfg");
    std::fs::write(&after, &renamed).unwrap();
    assert!(
        accepts(&bin, &after),
        "the renamed config must be accepted by the real parser:\n{renamed}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// G2 counterpart: the names the gate REJECTS really are the ones the
/// parser rejects unquoted — keeps the gate and reality glued.
#[test]
fn rejected_rename_targets_really_break_configs() {
    let Ok(bin) = std::env::var("KAMAILIO_LSP_TEST_BIN") else {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_BIN not set");
        return;
    };
    let dir = std::env::temp_dir().join(format!("kamlsp-rt-neg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for bad in ["a.b", "a:b", "a-b", "1ab"] {
        assert!(!logic::valid_route_name(bad), "{bad} must be gate-rejected");
        // what a rename WOULD have produced at an unquoted definition
        let cfg = format!(
            "#!KAMAILIO\nroute[{bad}] {{\n    exit;\n}}\nrequest_route {{\n    route({bad});\n}}\n"
        );
        let f = dir.join("bad.cfg");
        std::fs::write(&f, &cfg).unwrap();
        assert!(
            !accepts(&bin, &f),
            "'{bad}' is unquoted-legal after all — loosen the gate?"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
