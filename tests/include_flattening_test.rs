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
    let dir = std::env::temp_dir().join(format!("kamlsp-flatten-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
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
    let _ = std::fs::remove_dir_all(&dir);
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
