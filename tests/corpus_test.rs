//! Real-corpus sweep: the default cfg and every example cfg that
//! ships with Kamailio (etc/, misc/examples/, module examples), from
//! the tree in KAMAILIO_LSP_TEST_TREE.
//!
//! Old examples are KNOWN to carry syntax problems or reference
//! modules that are not installed, so validity is data, not an
//! assertion. The pinned invariants are:
//!   - the analyzer never panics on any corpus file
//!   - with KAMAILIO_LSP_TEST_BIN set: every file the real parser
//!     rejects yields at least one ERROR diagnostic (no silent
//!     failures), and every accepted file yields no ERROR (warnings
//!     are legitimate on rc=0)

mod common;

use kamailio_lsp::{analyze, diag};

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

#[test]
fn corpus_sweep_no_panics_and_no_silent_failures() {
    let tree = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let tree = std::path::PathBuf::from(tree);
    let files = corpus(&tree);
    assert!(
        files.len() >= 20,
        "expected a real corpus, found {} cfg files",
        files.len()
    );

    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
    let (mut ok, mut broken, mut mods_total, mut routes_total) = (0usize, 0usize, 0usize, 0usize);

    for f in &files {
        let text = String::from_utf8_lossy(&std::fs::read(f).unwrap()).into_owned();
        // invariant 1: the analyzer handles every real-world file
        let mods = analyze::loaded_modules(&text);
        let defs = analyze::route_defs(&text);
        let _ = analyze::route_refs(&text);
        mods_total += mods.len();
        routes_total += defs.len();

        // invariant 2: the formatter is safe on every real-world file
        // it will ever meet.  Fixtures cannot cover the layouts people
        // actually write — hanging argument indents, conditions broken
        // across lines, braceless `if` bodies — so the corpus is where
        // that gets proven.
        for opts in [
            kamailio_lsp::format::Options {
                insert_spaces: false,
                tab_size: 4,
            },
            kamailio_lsp::format::Options {
                insert_spaces: true,
                tab_size: 4,
            },
        ] {
            let out = kamailio_lsp::format::format(&text, &opts);
            assert_eq!(
                out,
                kamailio_lsp::format::format(&out, &opts),
                "formatting {} was not idempotent",
                f.display()
            );
            let skel =
                |t: &str| -> Vec<String> { t.lines().map(|l| l.trim().to_string()).collect() };
            assert_eq!(
                skel(&out),
                skel(&text),
                "formatting {} changed content, not just indentation",
                f.display()
            );
        }

        // invariant 3: real-parser rejection is never silent
        {
            let bin = &bin;
            let out = std::process::Command::new(bin)
                .args(["-c", "--all-errors", "-Y"])
                .arg(std::env::temp_dir())
                .arg("-f")
                .arg(f)
                // include_file with a relative path resolves against
                // the working directory
                .current_dir(f.parent().unwrap())
                .output()
                .expect("spawn kamailio -c");
            let rc = out.status.code().unwrap_or(-1);
            let all = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let diags = diag::parse_check_output(&all, rc);
            let errors: Vec<_> = diags
                .iter()
                .filter(|d| d.severity == diag::Severity::Error)
                .collect();
            if rc == 0 {
                ok += 1;
                assert!(
                    errors.is_empty(),
                    "{}: accepted by -c but ERROR diagnostics produced: {errors:?}",
                    f.display()
                );
            } else {
                broken += 1;
                assert!(
                    !diags.is_empty(),
                    "{}: rejected by -c (rc={rc}) but NO diagnostic surfaced — silent failure.\noutput:\n{}",
                    f.display(),
                    &all[..all.len().min(2000)]
                );
            }
        }
    }
    eprintln!(
        "corpus: {} files, {} loadmodules, {} route blocks, \
         -c: {ok} accepted / {broken} rejected (old examples are known-broken)",
        files.len(),
        mods_total,
        routes_total
    );
}
