//! Differential gates that keep the language model honest against the
//! real binary — the structural fix for model-vs-reality drift.
//!
//! Gated like the corpus sweep: KAMAILIO_LSP_TEST_TREE points at a
//! Kamailio source tree, KAMAILIO_LSP_TEST_BIN at a kamailio binary.

mod common;

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
///
/// Scope: the GRAMMAR-derived analyzer only.  Catalog-pinned checks
/// (`logic::catalog_diagnostics`) are deliberately excluded — they
/// assert doc coverage of the configured tree, not parser truth, and
/// a README gap on an accepted config is a documentation finding, not
/// an analyzer bug.  What the catalogue must contain is G3 below,
/// against every module in the tree rather than the handful the
/// corpus configs happen to load.
#[test]
fn analyzer_is_silent_on_every_accepted_corpus_config() {
    let tree = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
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
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
    let dir = std::env::temp_dir().join(format!("kamlsp-roundtrip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // a named route with bare AND quoted call sites
    let original = "#!KAMAILIO\nroute[OLD_NAME] {\n    exit;\n}\nrequest_route {\n    route(OLD_NAME);\n    route(\"OLD_NAME\");\n}\n";
    let before = dir.join("before.cfg");
    std::fs::write(&before, original).unwrap();
    // a binary that rejects the baseline makes the round-trip prove
    // nothing, so it is a failure rather than a reason to opt out
    assert!(
        accepts(&bin, &before),
        "the test binary rejects the baseline fixture, so the rename \
         round-trip would prove nothing"
    );

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
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
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

/// The README's own `modparam` examples, as ground truth for what the
/// catalogue must contain.
///
/// The harvester reads headings; the examples underneath them are
/// written by the same author for the same release, and they name the
/// parameter the way a configuration has to write it.  Where the two
/// disagree the harvester is normally the one that is wrong — that is
/// how a grouped chapter (`kazoo`), a chapter of its own
/// (`carrierroute`, `matrix`), an unparenthesised type (`ims_qos`)
/// and a restarted numbering (`rtpengine`) were each found, sixty-one
/// parameters between them, every one of which made the server warn
/// about a parameter that exists.
///
/// The other direction — an example naming a parameter the module does
/// not export — used to be a hand-curated array of 17 triples. It is
/// now derived: a module's `param_export_t` tables are the list
/// `modparam()` is checked against at startup, so a name absent from
/// them is not a parameter, whoever wrote it. That is provable rather
/// than curated, and it carries its own evidence.
/// G3: every parameter a module's own README sets in an example is in
/// the catalogue, unless the module does not export it at all.
#[test]
fn the_catalogue_contains_every_parameter_the_readmes_set() {
    let tree = common::required_env("KAMAILIO_LSP_TEST_TREE");
    let root = std::path::Path::new(&tree);
    let re =
        regex::Regex::new(r#"modparam\(\s*"([A-Za-z0-9_]+)"\s*,\s*"([A-Za-z0-9_]+)""#).unwrap();
    let catalogue = kamailio_lsp::catalog::builtin_modules();

    let mut checked = 0usize;
    let mut missing: Vec<String> = Vec::new();
    let mut not_exported: Vec<String> = Vec::new();
    let mut with_tables = 0usize;
    let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(root.join("src").join("modules"))
        .expect("a Kamailio source tree has src/modules")
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let module = dir.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(readme) = std::fs::read_to_string(dir.join("README")) else {
            continue;
        };
        let Some(doc) = catalogue.modules.iter().find(|m| m.name == module) else {
            missing.push(format!(
                "{module}: the module itself is not in the catalogue"
            ));
            continue;
        };
        let exported = kamailio_lsp::catalog::param_names_from_c(&dir, root);
        if !exported.names.is_empty() {
            with_tables += 1;
        }
        for c in re.captures_iter(&readme) {
            // only what the module says about ITSELF: a README often
            // shows another module's parameter in a worked example
            if c[1] != module {
                continue;
            }
            let param = c[2].to_string();
            checked += 1;
            if doc.params.iter().any(|p| p.name == param) {
                continue;
            }
            // absent from the catalogue is a failure unless the
            // module's own C tables say it is not a parameter at all
            if !exported.names.is_empty() && !exported.names.contains(&param) {
                let entry = format!("{module}::{param}");
                if !not_exported.contains(&entry) {
                    not_exported.push(entry);
                }
            } else {
                missing.push(format!("{module}::{param}"));
            }
        }
    }
    assert!(checked > 2000, "suspiciously few examples read: {checked}");
    // POSITIVE CONTROL. The exception is "absent from the C tables".
    // Were the extraction to stop matching, every example would be
    // excused and this test would pass having proven nothing.
    assert!(
        with_tables >= 220,
        "only {with_tables} modules exposed a parameter table; \
         the derived exception would excuse everything"
    );
    assert!(
        missing.is_empty(),
        "{} parameter(s) a README sets are exported by the module but absent from the catalogue:\n{}",
        missing.len(),
        missing.join("\n")
    );
    eprintln!(
        "catalogue covers {checked} README modparam examples; {} example(s) name a parameter the module never exported:\n{}",
        not_exported.len(),
        not_exported.join("\n")
    );
}
