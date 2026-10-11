//! The tree-sitter grammar must parse every configuration the real
//! parser accepts.
//!
//! Editors other than VS Code (Neovim, Helix, Zed) build their view of
//! the file from this grammar alone, so a construct it does not know
//! shows up there as an error in a valid config.
//!
//! The corpus is every `.cfg` that ships in the pinned Kamailio tree
//! (`etc/`, `misc/examples/`, module examples) that `kamailio -c` does
//! not reject with `syntax error` — module-loading failures are not the
//! grammar's business — plus the fixtures in `tests/grammar/`, which
//! hold constructs the shipped examples do not exercise and which the
//! binary must accept as written.
//!
//! A switched-off `#!ifdef` region is not Kamailio code —
//! `etc/kamailio.cfg` keeps SQL inside `#!ifdef ACCDB_COMMENT` — so the
//! lines the preprocessor skips are blanked before parsing, judged
//! from the config's own defines the way cfg.lex judges them for
//! `kamailio -c -f`.  `the_skip_emulation_agrees_with_the_parser`
//! holds that emulation to the binary.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

fn corpus(tree: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push = |d: PathBuf, recurse: bool| {
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
    push(tree.join("etc"), false);
    push(tree.join("misc").join("examples"), true);
    if let Ok(rd) = std::fs::read_dir(tree.join("src").join("modules")) {
        for e in rd.flatten() {
            push(e.path().join("examples"), false);
        }
    }
    out.sort();
    out
}

struct Kamailio {
    bin: String,
    mpath: String,
}

impl Kamailio {
    fn run(&self, f: &Path, args: &[&str]) -> (String, String) {
        let out = Command::new(&self.bin)
            .args(args)
            .arg("-L")
            .arg(&self.mpath)
            .arg("-f")
            .arg(f)
            .current_dir(f.parent().unwrap())
            .output()
            .expect("spawn kamailio");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// `kamailio -c` reported a syntax error in `f`.
    fn syntax_error(&self, f: &Path) -> bool {
        let rundir = std::env::temp_dir();
        let (o, e) = self.run(f, &["-c", "--all-errors", "-Y", rundir.to_str().unwrap()]);
        o.contains("syntax error") || e.contains("syntax error")
    }
}

/// `text` with every line the preprocessor skips left blank, line
/// numbers unchanged, judged as `kamailio -c -f` judges it with no
/// `-D`: a symbol is defined once a define-family directive in a live
/// region names it (cfg.lex only matches those in INITIAL state, never
/// in IFDEF_SKIP), `#!ifdef` / `#!ifndef` test that, `#!else` flips the
/// innermost level and `#!endif` closes it.  `#!ifexp` would need an
/// expression evaluator; no shipped config uses one, and meeting one
/// fails loudly rather than guessing.
fn as_the_parser_reads_it(text: &str) -> String {
    const DEFINING: &[&str] = &[
        "define",
        "def",
        "trydefine",
        "trydef",
        "redefine",
        "redef",
        "defexp",
        "defexps",
        "defenv",
        "defenvs",
        "trydefenv",
        "trydefenvs",
    ];
    let mut defined: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut levels: Vec<bool> = Vec::new();
    let mut out = Vec::new();
    for line in text.split('\n') {
        let live = levels.iter().all(|l| *l);
        let t = line.trim_start();
        let directive = t.strip_prefix("#!").or_else(|| t.strip_prefix("!!"));
        let mut words = directive.unwrap_or("").split_whitespace();
        let (kw, arg) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
        match (directive.is_some(), kw) {
            (true, "ifdef") => levels.push(defined.contains(arg)),
            (true, "ifndef") => levels.push(!defined.contains(arg)),
            (true, "else") => {
                if let Some(l) = levels.last_mut() {
                    *l = !*l;
                }
            }
            (true, "endif") => {
                levels.pop();
            }
            (true, "ifexp") => panic!("#!ifexp needs an evaluator this test does not have"),
            (true, k) if live && DEFINING.contains(&k) => {
                defined.insert(arg.to_string());
            }
            _ => {}
        }
        let is_conditional =
            directive.is_some() && matches!(kw, "ifdef" | "ifndef" | "else" | "endif");
        out.push(if live || is_conditional { line } else { "" });
    }
    out.join("\n")
}

/// The emulation above, held to the binary.  Each case's skipped text
/// is something the parser would reject if it read it — the variant
/// with the conditional lines removed must fail `kamailio -c` — so the
/// original passing proves those lines are skipped, and the emulation
/// must blank exactly them.
#[test]
fn the_skip_emulation_agrees_with_the_parser() {
    let k = Kamailio {
        bin: common::required_env("KAMAILIO_LSP_TEST_BIN"),
        mpath: common::required_env("KAMAILIO_LSP_TEST_MPATH"),
    };
    let dir = std::env::temp_dir().join(format!("kamlsp-skip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // (config, the lines that must come out blank)
    let cases: &[(&str, &[usize])] = &[
        (
            "request_route { exit; }\n#!ifdef NOPE\nthis is skipped\n#!endif\n",
            &[2],
        ),
        (
            "#!define WITH_A\n#!ifdef WITH_A\nrequest_route { exit; }\n#!else\nSELECT 'x\n#!endif\n",
            &[4],
        ),
        // a define inside a skipped region does not count
        (
            "#!define WITH_A\n#!ifndef WITH_A\n#!define LATE 1\n#!endif\n#!ifdef LATE\nthis is skipped\n#!endif\nrequest_route { exit; }\n",
            &[2, 5],
        ),
        // the short spellings define too
        (
            "#!def ON\n#!ifndef ON\nthis is skipped\n#!endif\nrequest_route { exit; }\n",
            &[2],
        ),
    ];
    for (i, (cfg, blank)) in cases.iter().enumerate() {
        let f = dir.join(format!("c{i}.cfg"));
        std::fs::write(&f, cfg).unwrap();
        assert!(
            !k.syntax_error(&f),
            "case {i}: the parser rejects it:\n{cfg}"
        );
        let unguarded: String = cfg
            .lines()
            .filter(|l| !l.starts_with("#!"))
            .map(|l| format!("{l}\n"))
            .collect();
        let g = dir.join(format!("c{i}-unguarded.cfg"));
        std::fs::write(&g, &unguarded).unwrap();
        assert!(
            k.syntax_error(&g),
            "case {i}: the skipped text parses anyway, so this case proves nothing"
        );
        let got: Vec<usize> = as_the_parser_reads_it(cfg)
            .lines()
            .zip(cfg.lines())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(n, _)| n)
            .collect();
        assert_eq!(&got, blank, "case {i}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_grammar_parses_every_config_the_real_parser_accepts() {
    let tree = PathBuf::from(common::required_env("KAMAILIO_LSP_TEST_TREE"));
    let k = Kamailio {
        bin: common::required_env("KAMAILIO_LSP_TEST_BIN"),
        mpath: common::required_env("KAMAILIO_LSP_TEST_MPATH"),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixtures: Vec<PathBuf> = {
        let mut v: Vec<PathBuf> = std::fs::read_dir(root.join("tests/grammar"))
            .expect("tests/grammar")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "cfg"))
            .collect();
        v.sort();
        v
    };
    assert!(!fixtures.is_empty(), "the fixtures went missing");
    for f in &fixtures {
        assert!(
            !k.syntax_error(f),
            "{}: a fixture the real parser rejects proves nothing",
            f.display()
        );
    }
    let (accepted, rejected): (Vec<PathBuf>, Vec<PathBuf>) =
        corpus(&tree).into_iter().partition(|f| !k.syntax_error(f));
    // POSITIVE CONTROL: a tree with nothing accepted makes this vacuous
    assert!(
        accepted.len() >= 40,
        "only {} shipped configs accepted ({} rejected)",
        accepted.len(),
        rejected.len()
    );

    // what the parser reads, one file per config, named after it
    let scratch = std::env::temp_dir().join(format!("kamlsp-sweep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let mut printed = Vec::new();
    for (i, f) in accepted.iter().chain(&fixtures).enumerate() {
        let raw = String::from_utf8_lossy(&std::fs::read(f).unwrap()).into_owned();
        let p = scratch.join(format!(
            "{i:03}-{}",
            f.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&p, as_the_parser_reads_it(&raw)).unwrap();
        printed.push((p, f.clone()));
    }
    let out = Command::new("npx")
        .args(["--yes", "tree-sitter-cli@0.26", "parse", "--quiet"])
        .args(printed.iter().map(|(p, _)| p))
        .current_dir(root.join("tree-sitter-kamailio"))
        .output()
        .expect("npx tree-sitter-cli parse");
    let _ = std::fs::remove_dir_all(&scratch);
    let report = String::from_utf8_lossy(&out.stdout);
    let failed: Vec<String> = report
        .lines()
        .filter(|l| l.contains("(ERROR") || l.contains("(MISSING"))
        .map(|l| {
            // name the original config, not the scratch copy
            printed
                .iter()
                .find(|(p, _)| l.starts_with(&*p.to_string_lossy()))
                .map(|(p, f)| l.replacen(&*p.to_string_lossy(), &f.to_string_lossy(), 1))
                .unwrap_or_else(|| l.to_string())
        })
        .collect();
    assert!(
        failed.is_empty(),
        "{} of {} configs the real parser accepts do not parse:\n{}",
        failed.len(),
        printed.len(),
        failed.join("\n")
    );
    eprintln!(
        "grammar parses {} accepted configs ({} rejected by the real parser skipped)",
        printed.len(),
        rejected.len()
    );
}

/// `queries/folds.scm` is read by editors, never by `tree-sitter test`,
/// so nothing else would notice it naming a node the grammar does not
/// have — the query then fails to load and folding silently stops.
/// On the highlight fixture it must fold both route blocks, the two
/// `if` blocks and the nested block comment.
#[test]
fn the_fold_query_loads_and_folds_blocks_and_comments() {
    let grammar = Path::new(env!("CARGO_MANIFEST_DIR")).join("tree-sitter-kamailio");
    let out = Command::new("npx")
        .args([
            "--yes",
            "tree-sitter-cli@0.26",
            "query",
            "queries/folds.scm",
            "test/highlight/basics.cfg",
        ])
        .current_dir(&grammar)
        .output()
        .expect("npx tree-sitter-cli query");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "folds.scm does not load: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        stdout.matches("capture:").count(),
        5,
        "expected 5 folds:\n{stdout}"
    );
}
