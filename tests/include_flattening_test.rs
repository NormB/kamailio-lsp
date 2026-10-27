//! What the analyzer follows as an include must be what Kamailio
//! opens — and Kamailio's answer is not OpenSIPS's.
//!
//! Kamailio's preprocessor is part of the lexer: `cfg.lex` has rules
//! for `include_file`, `import_file` and both `PREP_START`-prefixed
//! forms (`#!`, `!!`), and because it lexes, a `/* */` block really
//! does suppress the directive inside it. OpenSIPS flattens line by
//! line before lexing and behaves the opposite way on every one of
//! those points. The two are measured against their own binaries
//! rather than assumed to match.
//!
//! One trap is written into the probe. Absence is NOT a usable signal
//! for `import_file`: it fires and then tolerates a missing file in
//! silence, so a "did it try to open a file that is not there" test
//! reports it as ignored when it is not. The probe therefore includes
//! a file that EXISTS and is invalid, and asks whether the error
//! surfaces.

mod common;

use kamailio_lsp::analyze::includes;

/// `{}` is replaced by a path to a file that exists and is invalid.
const SHAPES: &[(&str, &str)] = &[
    (
        "bare include_file",
        "include_file \"{}\"\nrequest_route {{ exit; }}\n",
    ),
    (
        "bare import_file",
        "import_file \"{}\"\nrequest_route {{ exit; }}\n",
    ),
    (
        "#! prefixed",
        "#!include_file \"{}\"\nrequest_route {{ exit; }}\n",
    ),
    (
        "!! prefixed",
        "!!include_file \"{}\"\nrequest_route {{ exit; }}\n",
    ),
    (
        "own line inside a block comment",
        "/*\ninclude_file \"{}\"\n*/\nrequest_route {{ exit; }}\n",
    ),
    (
        "inside a call on one line",
        "request_route {{\nxlog(\"include_file \\\"{}\\\"\");\nexit;\n}}\n",
    ),
];

/// Whether the real checker surfaces the included file's own error —
/// which it can only do if it opened and parsed it.
fn the_real_parser_reads_it(
    bin: &str,
    mpath: &str,
    rt: &std::path::Path,
    dir: &std::path::Path,
    tag: &str,
    cfg: &str,
) -> bool {
    let path = dir.join(format!("{tag}.cfg"));
    std::fs::write(&path, cfg).expect("fixture config");
    let mut cmd = std::process::Command::new(bin);
    cmd.arg("-c").arg("--all-errors").arg("-Y").arg(rt);
    if !mpath.is_empty() {
        cmd.arg("-L").arg(mpath);
    }
    let out = cmd.arg("-f").arg(&path).output().expect("the checker runs");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    all.contains("broken-include.inc")
}

#[test]
fn the_analyzer_sees_exactly_what_kamailio_reads() {
    let bin = common::required_env("KAMAILIO_LSP_TEST_BIN");
    let mpath = common::required_env("KAMAILIO_LSP_TEST_MPATH");
    let scratch = common::Scratch::new("kamlsp-flatten");
    let dir = scratch.path().to_path_buf();
    std::fs::create_dir_all(dir.join("rt")).unwrap();

    // exists, and is invalid: absence would not discriminate, because
    // `import_file` fires and then tolerates a missing file silently
    let broken = dir.join("broken-include.inc");
    std::fs::write(&broken, "@@@ not valid kamailio @@@\n").unwrap();

    let mut disagreements: Vec<String> = Vec::new();
    let mut read = 0usize;
    for (label, template) in SHAPES {
        let cfg = template.replace("{}", &broken.display().to_string());
        let tag: String = label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        let real = the_real_parser_reads_it(&bin, &mpath, &dir.join("rt"), &dir, &tag, &cfg);
        let seen = includes(&cfg)
            .iter()
            .any(|i| i.name.contains("broken-include"));
        if real {
            read += 1;
        }
        if real != seen {
            disagreements.push(format!(
                "  {label}: Kamailio {} the file, the analyzer {} it",
                if real { "READS" } else { "ignores" },
                if seen { "follows" } else { "misses" }
            ));
        }
    }

    // POSITIVE CONTROL: if nothing was read, every comparison would be
    // "neither" and this would pass having proved nothing.
    assert!(
        read >= 4,
        "only {read} of {} shapes were read by the real checker",
        SHAPES.len()
    );
    assert!(
        disagreements.is_empty(),
        "the analyzer and Kamailio disagree on {} shape(s):\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
}

/// The two languages differ here, and the difference is the point:
/// stated directly so a failure says which half moved.
#[test]
fn a_block_comment_suppresses_the_directive_unlike_opensips() {
    assert!(
        includes("/*\ninclude_file \"x.cfg\"\n*/\n").is_empty(),
        "Kamailio lexes its preprocessor, so a block comment really does suppress it"
    );
    for text in [
        "#!include_file \"x.cfg\"\n",
        "!!include_file \"x.cfg\"\n",
        "import_file \"x.cfg\"\n",
    ] {
        assert_eq!(
            includes(text).len(),
            1,
            "Kamailio reads this form: {text:?}"
        );
    }
}

/// How far the include closure follows, against Kamailio's own limits.
///
/// `src/core/cfg.lex` at tag 6.1.5, line 102: `#define MAX_INCLUDE_DEPTH
/// 10`; `sr_push_yy_state` (line 1874) refuses with "too many includes"
/// once `include_stack_ptr` reaches it. The stack holds only included
/// files, not the main one. Measured with `kamailio -c` 6.1.5: a route
/// defined 10 nested includes below the root is read (rc 0), and one
/// more level fails with "too many includes" (rc 255), for
/// `include_file` and `import_file` alike. Nothing in cfg.lex counts
/// files: a root including 600 files is accepted (rc 0).
///
/// The closure stopped at 8 levels and 64 files, so a route defined
/// deeper or later read as "not defined". Kamailio sets no file count;
/// the closure's bound is the editor's own, the same 500 the workspace
/// sweep reads, so the closure never sees past what the include graph
/// can.
#[test]
fn the_include_closure_follows_as_far_as_kamailio_reads() {
    use kamailio_lsp::logic::{analyzer_diagnostics, include_closure};
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    let run = |files: &HashMap<PathBuf, String>| {
        let root = Path::new("/t/kamailio.cfg");
        let loader = |p: &Path| files.get(p).cloned();
        let text = files[root].clone();
        let closure = include_closure(root, &text, &loader);
        let warnings: Vec<String> = analyzer_diagnostics(root, &text, &loader)
            .into_iter()
            .map(|d| d.message)
            .collect();
        (closure.len(), warnings)
    };
    // root -> f1 -> ... -> f{n}; f{n} defines route[DEEP]
    let chain = |n: usize| {
        let mut m = HashMap::new();
        m.insert(
            PathBuf::from("/t/kamailio.cfg"),
            "include_file \"f1.cfg\"\nrequest_route {\n    route(DEEP);\n}\n".to_string(),
        );
        for i in 1..=n {
            let body = if i < n {
                format!("include_file \"f{}.cfg\"\n", i + 1)
            } else {
                "route[DEEP] {\n    exit;\n}\n".to_string()
            };
            m.insert(PathBuf::from(format!("/t/f{i}.cfg")), body);
        }
        m
    };
    // root includes w1 .. w{n}; each defines route[W{i}], all called
    let wide = |n: usize| {
        let mut root = String::new();
        let mut calls = String::new();
        let mut m = HashMap::new();
        for i in 1..=n {
            root.push_str(&format!("include_file \"w{i}.cfg\"\n"));
            calls.push_str(&format!("    route(W{i});\n"));
            m.insert(
                PathBuf::from(format!("/t/w{i}.cfg")),
                format!("route[W{i}] {{\n    exit;\n}}\n"),
            );
        }
        root.push_str(&format!("request_route {{\n{calls}}}\n"));
        m.insert(PathBuf::from("/t/kamailio.cfg"), root);
        m
    };

    let mut wrong = Vec::new();
    let (n, w) = run(&chain(10));
    if n != 11 || !w.is_empty() {
        wrong.push(format!("10 levels: {n} files, warnings {w:?}"));
    }
    // negative control: the 11th level is past what Kamailio reads
    let (n, w) = run(&chain(11));
    if n != 11 || !w.iter().any(|m| m.contains("DEEP")) {
        wrong.push(format!("11 levels: {n} files, warnings {w:?}"));
    }
    let (n, w) = run(&wide(100));
    if n != 101 || !w.is_empty() {
        wrong.push(format!("100 siblings: {n} files, {} warnings", w.len()));
    }
    // the editor's bound, shared with the workspace sweep
    let (n, _) = run(&wide(600));
    if n != 500 {
        wrong.push(format!("600 siblings: {n} files, want the 500-file bound"));
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}
