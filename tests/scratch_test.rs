//! Every scratch path a test makes under the system temp directory is
//! removed when the test ends, whether it passed, failed an assertion
//! or panicked.
//!
//! Cleanup used to be the last statement of each test, the one line a
//! failing assertion never reaches, so every red test left its
//! directory behind. `common::Scratch` ties the removal to scope; the
//! gate at the end keeps every test on it.

mod common;
use common::Scratch;
use std::path::{Path, PathBuf};

#[test]
fn a_scratch_directory_is_removed_when_its_guard_goes_out_of_scope() {
    let path: PathBuf;
    {
        let scratch = Scratch::new("kamlsp-scratch-pass");
        let file = scratch.join("sub").join("f.txt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "x").unwrap();
        // POSITIVE CONTROL: the file was really written
        assert!(file.is_file(), "{} was not created", file.display());
        path = scratch.path().to_path_buf();
    }
    assert!(
        !path.exists(),
        "{} survived the end of its guard's scope",
        path.display()
    );
}

#[test]
fn a_scratch_directory_is_removed_when_the_test_panics() {
    let mut recorded: Option<PathBuf> = None;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let scratch = Scratch::new("kamlsp-scratch-fail");
        std::fs::create_dir_all(scratch.path()).unwrap();
        std::fs::write(scratch.join("f.txt"), "x").unwrap();
        recorded = Some(scratch.path().to_path_buf());
        panic!("a failing assertion");
    }));
    assert!(outcome.is_err(), "the closure must have panicked");
    let path = recorded.expect("the path was recorded before the panic");
    assert!(
        !path.exists(),
        "{} survived a panic while its guard was held",
        path.display()
    );
}

#[test]
fn a_scratch_path_written_as_a_plain_file_is_removed() {
    let path: PathBuf;
    {
        let scratch = Scratch::new("kamlsp-scratch-file");
        std::fs::write(scratch.path(), "x").unwrap();
        // POSITIVE CONTROL: the path is a file, not a directory
        assert!(scratch.is_file(), "{} was not written", scratch.display());
        path = scratch.path().to_path_buf();
    }
    assert!(
        !path.exists(),
        "{} survived the end of its guard's scope",
        path.display()
    );
}

fn rust_files(dir: &Path, skip: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p == skip {
            continue;
        }
        if p.is_dir() {
            rust_files(&p, skip, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// `tests/common/` holds `Scratch` itself, the one place allowed to
/// build a path under the system temp directory.
#[test]
fn no_test_builds_a_temp_path_without_the_guard() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files = Vec::new();
    rust_files(&tests, &tests.join("common"), &mut files);
    files.sort();
    // POSITIVE CONTROL: the walk found the test files
    assert!(files.len() > 50, "only {} test files found", files.len());
    // split so this file does not match itself
    let needle = concat!("temp", "_dir(");
    let mut sites = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        for (i, line) in text.lines().enumerate() {
            if line.contains(needle) {
                let rel = f.strip_prefix(env!("CARGO_MANIFEST_DIR")).unwrap();
                sites.push(format!("{}:{}", rel.display(), i + 1));
            }
        }
    }
    assert!(
        sites.is_empty(),
        "{} test lines build a path under the system temp directory \
         without common::Scratch, so a failing test leaves it behind:\n{}",
        sites.len(),
        sites.join("\n")
    );
}
