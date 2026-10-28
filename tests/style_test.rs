//! Style gate: the Google Rust Style Guide
//! (<https://google.github.io/styleguide/rustguide.html>), with the
//! official Rust Style Guide and the Rust API Guidelines it defers to.
//!
//! rustfmt and clippy enforce most of it. This file checks that both stay
//! wired up, and it checks the rules that neither of them can see:
//!
//! 1. `rustfmt.toml` carries the guide's options, and CI formats with
//!    `scripts/fmt.sh`, whose pinned nightly reads them;
//! 2. both packages deny the guide's lints, and CI and `scripts/gate.sh`
//!    run clippy on both with warnings as errors;
//! 3. the git hooks run `scripts/gate.sh`, and CI runs their tests;
//! 4. a `#[rustfmt::skip]` has the reason on the line above it;
//! 5. no glob import outside the guide's exceptions, in any file;
//! 6. no `catch_unwind` outside the tests, no unstable `#![feature]`, and
//!    no crate that runs code before `main`.
//!
//! Each rule is a function of the text it reads, so the tests at the end
//! drive it with text that breaks the rule as well as the real files.

use std::collections::BTreeMap;
use std::path::Path;

/// The options `rustfmt.toml` must set, and their values.
const RUSTFMT_OPTIONS: &[(&str, &str)] = &[
    ("style_edition", "\"2024\""),
    ("max_width", "100"),
    ("hex_literal_case", "\"Lower\""),
    ("format_macro_matchers", "true"),
    ("imports_granularity", "\"Module\""),
];

/// The lints each package must deny, as `table.lint`.
const DENIED_LINTS: &[&str] = &[
    "clippy.borrowed_box",
    "clippy.ptr_arg",
    "clippy.should_implement_trait",
    "clippy.wrong_self_convention",
    "clippy.new_without_default",
    "clippy.len_without_is_empty",
    "clippy.undocumented_unsafe_blocks",
    "clippy.unwrap_used",
    "clippy.wildcard_imports",
];

/// Crates that run code before `main`, which the guide forbids.
const BEFORE_MAIN_CRATES: &[&str] = &["ctor", "inventory", "linkme", "static_init"];

/// Whether a line of a shell script, a workflow or a Markdown file that is
/// not a `#` comment contains `needle`. A comment that names a command
/// does not run it.
fn runs(text: &str, needle: &str) -> bool {
    text.lines()
        .any(|l| !l.trim_start().starts_with('#') && l.contains(needle))
}

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()))
}

/// The `key = value` lines of a TOML file, by `table.key`, with comments
/// and surrounding space removed. Enough for the flat files read here.
fn toml_values(text: &str) -> BTreeMap<String, String> {
    let mut table = String::new();
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.split_once('#').map_or(line, |(code, _)| code).trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = name.trim().to_string();
        } else if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let full = if table.is_empty() {
                key.to_string()
            } else {
                format!("{table}.{key}")
            };
            out.insert(full, value.trim().to_string());
        }
    }
    out
}

/// What is wrong with a `rustfmt.toml`: each option missing or set to
/// something else.
fn rustfmt_problems(text: &str) -> Vec<String> {
    let values = toml_values(text);
    RUSTFMT_OPTIONS
        .iter()
        .filter(|(key, want)| values.get(*key).map(String::as_str) != Some(*want))
        .map(|(key, want)| format!("rustfmt.toml must set {key} = {want}"))
        .collect()
}

/// The lints in [`DENIED_LINTS`] that a manifest does not deny.
fn undenied_lints(manifest: &str) -> Vec<&'static str> {
    let values = toml_values(manifest);
    DENIED_LINTS
        .iter()
        .copied()
        .filter(|lint| {
            let value = values.get(&format!("lints.{lint}")).map(String::as_str);
            !matches!(value, Some("\"deny\"" | "\"forbid\""))
        })
        .collect()
}

/// The lines of a Rust file with a `#[rustfmt::skip]` whose line above is
/// not a `//` comment giving the reason.
fn unexplained_skips(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            let code = line.trim_start();
            code.starts_with("#[rustfmt::skip") || code.starts_with("#![rustfmt::skip")
        })
        .filter(|(n, _)| {
            let above = n.checked_sub(1).map(|i| lines[i].trim_start());
            !above.is_some_and(|l| l.starts_with("//") && !l.starts_with("///"))
        })
        .map(|(n, _)| n + 1)
        .collect()
}

/// The lines of a Rust file that call `catch_unwind` or enable an
/// unstable feature.
fn forbidden_constructs(text: &str) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        if code.contains("catch_unwind") {
            out.push((n + 1, "catch_unwind"));
        }
        if code.starts_with("#![feature") {
            out.push((n + 1, "an unstable feature"));
        }
    }
    out
}

/// The lines of a Rust file with a glob import the guide does not allow.
/// It allows `pub use sub::*` (re-exports from a non-leaf module), and,
/// indented inside an item, `use super::*` (an inline module) and
/// `use Enum::*` (a function about that enum). clippy's wildcard_imports
/// does not look inside test code, so this reads every file.
fn glob_imports(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let code = line.trim_start();
        let indented = code.len() < line.len();
        let Some(path) = code
            .strip_prefix("use ")
            .and_then(|p| p.strip_suffix("::*;"))
        else {
            if code.starts_with("use ") && code.contains('*') {
                out.push(n + 1);
            }
            continue;
        };
        let last = path.rsplit("::").next().unwrap_or(path);
        let enum_like = last.starts_with(|c: char| c.is_ascii_uppercase());
        if !(indented && (path == "super" || enum_like)) {
            out.push(n + 1);
        }
    }
    out
}

/// The packages in a `Cargo.lock` that run code before `main`.
fn before_main_packages(lock: &str) -> Vec<String> {
    lock.lines()
        .filter_map(|l| l.strip_prefix("name = \""))
        .filter_map(|l| l.strip_suffix('"'))
        .filter(|name| BEFORE_MAIN_CRATES.contains(name))
        .map(str::to_string)
        .collect()
}

/// Every `.rs` file under `rel`, relative to the repository.
fn rust_files(rel: &str) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{} is readable: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("a directory entry is readable").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "target") {
                    walk(root, &path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                let rel = path
                    .strip_prefix(root)
                    .expect("the walk stays under the root");
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    walk(root, &root.join(rel), &mut out);
    out.sort();
    out
}

#[test]
fn rustfmt_toml_sets_the_guides_options() {
    let problems = rustfmt_problems(&read("rustfmt.toml"));
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn ci_formats_with_the_pinned_nightly() {
    let ci = read(".github/workflows/ci.yml");
    // Stable rustfmt ignores the unstable options, so a stable
    // `cargo fmt --check` passes a file the options would change.
    assert!(
        runs(&ci, "scripts/fmt.sh check"),
        "the rustfmt step must run scripts/fmt.sh check"
    );
    assert!(
        !runs(&ci, "cargo fmt --check") && !runs(&ci, "cargo fmt --manifest-path"),
        "a stable cargo fmt step does not apply rustfmt.toml's unstable options"
    );
    assert!(
        runs(
            &ci,
            "rustup toolchain install \"$(scripts/fmt.sh toolchain)\""
        ),
        "CI must install the toolchain scripts/fmt.sh names, not one of its own"
    );
}

#[test]
fn fmt_script_pins_a_dated_nightly() {
    let script = read("scripts/fmt.sh");
    let line = script
        .lines()
        .find_map(|l| l.strip_prefix("TOOLCHAIN="))
        .expect("scripts/fmt.sh sets TOOLCHAIN=");
    let date = line
        .strip_prefix("nightly-")
        .expect("the toolchain is a nightly");
    let parts: Vec<&str> = date.split('-').collect();
    assert!(
        parts.len() == 3
            && [4, 2, 2].iter().zip(&parts).all(|(n, p)| p.len() == *n)
            && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())),
        "the nightly is pinned by date, nightly-YYYY-MM-DD: {line}"
    );
}

#[test]
fn both_packages_deny_the_guides_lints() {
    for manifest in ["Cargo.toml", "fuzz/Cargo.toml"] {
        let missing = undenied_lints(&read(manifest));
        assert!(missing.is_empty(), "{manifest} does not deny {missing:?}");
    }
}

#[test]
fn ci_and_the_local_gate_run_clippy_on_both_packages_with_warnings_as_errors() {
    let ci = read(".github/workflows/ci.yml");
    // The gate adds --quiet; otherwise the two run the same commands.
    let gate = read("scripts/gate.sh").replace(" --quiet", "");
    for command in [
        "cargo clippy --all-targets -- -D warnings",
        "cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings",
        "cargo test --all-targets",
        "RUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps",
    ] {
        assert!(runs(&ci, command), "CI must run `{command}`");
        let gate_form = command.replace("RUSTDOCFLAGS=", "env RUSTDOCFLAGS=");
        assert!(
            runs(&gate, command) || runs(&gate, &gate_form),
            "scripts/gate.sh must run `{command}`, as CI does"
        );
    }
}

#[test]
fn the_hooks_run_the_gate_and_ci_tests_the_hooks() {
    let wants = [
        (".githooks/commit-msg", "scripts/attribution-scan.py"),
        (".githooks/pre-commit", "scripts/attribution-scan.py"),
        (".githooks/pre-commit", "scripts/gate.sh commit"),
        (".githooks/pre-push", "git verify-commit"),
        (".githooks/pre-push", "scripts/attribution-scan.py"),
        (".githooks/pre-push", "scripts/gate.sh push"),
        (".github/workflows/ci.yml", "run: scripts/test-hooks.sh"),
        ("CONTRIBUTING.md", "git config core.hooksPath .githooks"),
    ];
    for (file, text) in wants {
        assert!(
            runs(&read(file), text),
            "{file} must contain `{text}` outside a comment"
        );
    }
    let ci = read(".github/workflows/ci.yml");
    let needs = ci
        .split_once("\n  ci-success:")
        .and_then(|(_, job)| job.lines().find(|l| l.trim_start().starts_with("needs:")))
        .expect("ci.yml has a ci-success job with needs");
    assert!(
        needs.contains(" hooks]") || needs.contains(" hooks,"),
        "ci-success must wait for hooks: {needs}"
    );
}

#[test]
fn every_rustfmt_skip_gives_its_reason() {
    let mut offenders = Vec::new();
    for dir in ["src", "examples", "tests", "fuzz/fuzz_targets"] {
        for file in rust_files(dir) {
            for line in unexplained_skips(&read(&file)) {
                offenders.push(format!("{file}:{line}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "#[rustfmt::skip] is for deliberate alignment or generated code, \
         with the reason in a comment on the line above: {offenders:#?}"
    );
}

#[test]
fn no_catch_unwind_or_unstable_feature_outside_the_tests() {
    let mut offenders = Vec::new();
    for dir in ["src", "examples", "fuzz/fuzz_targets"] {
        for file in rust_files(dir) {
            for (line, what) in forbidden_constructs(&read(&file)) {
                offenders.push(format!("{file}:{line}: {what}"));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}

#[test]
fn no_test_enables_an_unstable_feature() {
    let mut offenders = Vec::new();
    for file in rust_files("tests") {
        for (line, what) in forbidden_constructs(&read(&file)) {
            // The tests may test what a panic leaves behind.
            if what != "catch_unwind" {
                offenders.push(format!("{file}:{line}: {what}"));
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}

#[test]
fn no_glob_import_the_guide_does_not_allow() {
    let mut offenders = Vec::new();
    for dir in ["src", "examples", "tests", "fuzz/fuzz_targets"] {
        for file in rust_files(dir) {
            for line in glob_imports(&read(&file)) {
                offenders.push(format!("{file}:{line}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "import the names, or the module for its functions: {offenders:#?}"
    );
}

#[test]
fn glob_rule_allows_only_the_guides_exceptions() {
    let text = "use common::*;\nuse std::io::*;\npub use sub::*;\nmod tests {\n    use super::*;\n}\nfn f() {\n    use Kind::*;\n    use std::fmt::*;\n}\nuse a::{b, c::*};\n";
    assert_eq!(
        glob_imports(text),
        vec![1, 2, 9, 11],
        "a top-level glob, a glob of a module inside a function and a glob in braces"
    );
    assert_eq!(
        glob_imports("use super::*;\n"),
        vec![1],
        "use super::* only in an inline module"
    );
    assert_eq!(
        glob_imports("use common::Server;\n"),
        Vec::<usize>::new(),
        "named imports"
    );
}

#[test]
fn no_dependency_runs_code_before_main() {
    for lock in ["Cargo.lock", "fuzz/Cargo.lock"] {
        let found = before_main_packages(&read(lock));
        assert!(
            found.is_empty(),
            "{lock} pulls in {found:?}, which run code before main"
        );
    }
}

// The rules on text that breaks them. A rule that cannot fail proves
// nothing by passing on the real files.

#[test]
fn rustfmt_rule_names_a_missing_or_changed_option() {
    let full: String = RUSTFMT_OPTIONS
        .iter()
        .map(|(k, v)| format!("{k} = {v}\n"))
        .collect();
    assert_eq!(
        rustfmt_problems(&full),
        Vec::<String>::new(),
        "every option set"
    );
    let changed = full.replace("max_width = 100", "max_width = 120");
    assert_eq!(
        rustfmt_problems(&changed),
        vec!["rustfmt.toml must set max_width = 100".to_string()],
        "a changed value is named"
    );
    let dropped = full.replace("imports_granularity = \"Module\"\n", "");
    assert_eq!(
        rustfmt_problems(&dropped),
        vec!["rustfmt.toml must set imports_granularity = \"Module\"".to_string()],
        "a missing option is named"
    );
    let commented = full.replace("hex_literal_case", "# hex_literal_case");
    assert_eq!(
        rustfmt_problems(&commented).len(),
        1,
        "a commented-out option is missing"
    );
}

#[test]
fn lint_rule_names_a_lint_that_is_not_denied() {
    let lint_line = |lint: &str, level: &str| {
        let name = lint.split_once('.').map_or(lint, |(_, n)| n);
        format!("{name} = \"{level}\"\n")
    };
    let all: String = DENIED_LINTS.iter().map(|l| lint_line(l, "deny")).collect();
    let manifest = format!("[package]\nname = \"x\"\n[lints.clippy]\n{all}");
    assert_eq!(
        undenied_lints(&manifest),
        Vec::<&str>::new(),
        "every lint denied"
    );
    let warned = manifest.replace("ptr_arg = \"deny\"", "ptr_arg = \"warn\"");
    assert_eq!(
        undenied_lints(&warned),
        vec!["clippy.ptr_arg"],
        "warn is not deny"
    );
    let elsewhere = manifest.replace("[lints.clippy]", "[lints.rust]");
    assert_eq!(
        undenied_lints(&elsewhere).len(),
        DENIED_LINTS.len(),
        "a clippy lint under [lints.rust] is not a clippy lint"
    );
}

#[test]
fn skip_rule_wants_a_plain_comment_on_the_line_above() {
    let explained =
        "// Aligned by hand: one column per field.\n#[rustfmt::skip]\nconst A: u8 = 1;\n";
    assert_eq!(
        unexplained_skips(explained),
        Vec::<usize>::new(),
        "explained"
    );
    let bare = "fn a() {}\n#[rustfmt::skip]\nconst A: u8 = 1;\n";
    assert_eq!(unexplained_skips(bare), vec![2], "no comment above");
    let doc = "/// A constant.\n#[rustfmt::skip]\nconst A: u8 = 1;\n";
    assert_eq!(
        unexplained_skips(doc),
        vec![2],
        "a doc comment documents the item, not the skip"
    );
    assert_eq!(
        unexplained_skips("#[rustfmt::skip]\n"),
        vec![1],
        "first line of the file"
    );
    let named = "let s = \"#[rustfmt::skip]\";\n// `#[rustfmt::skip]` in prose\n";
    assert_eq!(
        unexplained_skips(named),
        Vec::<usize>::new(),
        "a string or a comment is no attribute"
    );
}

#[test]
fn a_command_in_a_comment_does_not_run() {
    assert!(
        runs(
            "steps:\n  - run: scripts/fmt.sh check\n",
            "scripts/fmt.sh check"
        ),
        "a run line"
    );
    assert!(
        runs(
            "  run: |\n    scripts/fmt.sh check\n",
            "scripts/fmt.sh check"
        ),
        "a run block"
    );
    assert!(
        !runs("# then scripts/fmt.sh check\n", "scripts/fmt.sh check"),
        "a comment"
    );
    assert!(
        !runs("    # scripts/fmt.sh check\n", "scripts/fmt.sh check"),
        "an indented comment"
    );
}

#[test]
fn forbidden_rule_finds_catch_unwind_and_features_but_not_comments() {
    let text = "#![feature(never_type)]\nfn a() {\n    let _ = std::panic::catch_unwind(|| ());\n    // catch_unwind in a comment\n}\n";
    assert_eq!(
        forbidden_constructs(text),
        vec![(1, "an unstable feature"), (3, "catch_unwind")],
        "both constructs, with their lines; the comment is not code"
    );
    assert_eq!(
        forbidden_constructs("fn a() {}\n"),
        Vec::new(),
        "clean text"
    );
}

#[test]
fn before_main_rule_reads_package_names() {
    let lock = "[[package]]\nname = \"ctor\"\nversion = \"0.2.0\"\n\n[[package]]\nname = \"ctor-lite-ish\"\n";
    assert_eq!(
        before_main_packages(lock),
        vec!["ctor".to_string()],
        "exact names only"
    );
}
