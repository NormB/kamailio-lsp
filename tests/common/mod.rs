//! Shared stdio-LSP test harness.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::Child;
use std::sync::mpsc;
use std::time::Duration;

pub fn write_msg(w: &mut impl Write, v: &serde_json::Value) {
    let s = v.to_string();
    write!(w, "Content-Length: {}\r\n\r\n{}", s.len(), s).expect("the server reads its stdin");
    w.flush().expect("the server reads its stdin");
}

pub fn spawn_reader(child: &mut Child) -> mpsc::Receiver<serde_json::Value> {
    let stdout = child
        .stdout
        .take()
        .expect("the server was spawned with a piped stdout");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut r = BufReader::new(stdout);
        loop {
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let t = line.trim();
                if t.is_empty() {
                    break;
                }
                if let Some(v) = t.strip_prefix("Content-Length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
            }
            let mut buf = vec![0u8; len];
            if r.read_exact(&mut buf).is_err() {
                return;
            }
            if let Ok(v) = serde_json::from_slice(&buf)
                && tx.send(v).is_err()
            {
                return;
            }
        }
    });
    rx
}

pub fn wait_for<F: Fn(&serde_json::Value) -> bool>(
    rx: &mpsc::Receiver<serde_json::Value>,
    pred: F,
    what: &str,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let left = deadline
            .checked_duration_since(std::time::Instant::now())
            .unwrap_or_else(|| panic!("timeout waiting for {what}"));
        let v = rx
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("timeout waiting for {what}"));
        if pred(&v) {
            return v;
        }
    }
}

/// The proof environment is REQUIRED, not optional: under this
/// project's rule a skipped test IS a failed test.  A suite that
/// quietly opts out on a machine without a real tree or binary is a
/// suite that reports green while proving nothing.
///
/// `scripts/proof-env.sh` provisions what these tests need, and CI
/// runs that same script — the gate and its fixer derive from one
/// rule, so a green CI means the proofs actually ran.
pub fn required_env(var: &str) -> String {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => v,
        _ => panic!(
            "{var} is unset or empty.  These tests run against a real \
             tree and binary and are never skipped: run \
             `scripts/proof-env.sh` and export what it prints (CI runs \
             the same script)."
        ),
    }
}

/// A spawned server that owns the fixture directory it was given.
///
/// Every stdio test used to kill its child and remove its directory on
/// the last line of the test body — which is the one line a failing
/// assertion never reaches.  A green run left 27 directories behind; a
/// red one also left a server process running, and one of those was
/// found alive nearly six hours later.  Cleanup that only happens when
/// nothing goes wrong is not cleanup, so it belongs in `Drop`, which
/// runs on the panic path too.
///
/// `Deref` keeps every existing call site working: `spawn_reader(&mut
/// child)` and `child.stdin.take()` still mean what they meant, and an
/// explicit `child.kill()` at the end of a test is now belt-and-braces
/// rather than the only thing standing between a failure and a leak.
pub struct Server {
    child: Child,
    dir: std::path::PathBuf,
}

impl Server {
    pub fn new(child: Child, dir: impl AsRef<std::path::Path>) -> Self {
        Self {
            child,
            dir: dir.as_ref().to_path_buf(),
        }
    }

    /// Shadows `Child::kill` so the existing `let _ = child.kill();`
    /// lines keep compiling and keep meaning "stop it now".
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }
}

impl std::ops::Deref for Server {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}

impl std::ops::DerefMut for Server {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        // reap it, or the process lingers as a zombie for as long as
        // the test binary runs
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

use std::path::{Path, PathBuf};
use std::process::Command;

/// Every `.cfg` that ships in the pinned Kamailio tree: `etc/`,
/// `misc/examples/` and each module's `examples/`, sorted.
pub fn corpus(tree: &Path) -> Vec<PathBuf> {
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

/// The pinned binary and its module path.
pub struct Kamailio {
    pub bin: String,
    pub mpath: String,
}

impl Kamailio {
    pub fn run(&self, f: &Path, args: &[&str]) -> (String, String) {
        let out = Command::new(&self.bin)
            .args(args)
            .arg("-L")
            .arg(&self.mpath)
            .arg("-f")
            .arg(f)
            .current_dir(f.parent().expect("the path has a parent directory"))
            .output()
            .expect("spawn kamailio");
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// `kamailio -c` reported a syntax error in `f`.
    pub fn syntax_error(&self, f: &Path) -> bool {
        let rundir = check_rundir();
        let (o, e) = self.run(
            f,
            &[
                "-c",
                "--all-errors",
                "-Y",
                rundir.to_str().expect("the run directory path is UTF-8"),
            ],
        );
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
pub fn as_the_parser_reads_it(text: &str) -> String {
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

/// One corpus case: name, input text, expected S-expression text.
pub struct Case {
    pub name: String,
    pub input: String,
    pub expected: String,
}

/// Parse the tree-sitter corpus format: `===` fence, name, `===`
/// fence, input, `---` fence, expected S-expression, repeat.
pub fn parse_corpus(text: &str) -> Vec<Case> {
    let is_eq = |l: &str| l.len() >= 3 && l.chars().all(|c| c == '=');
    let is_dash = |l: &str| l.len() >= 3 && l.chars().all(|c| c == '-');
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !is_eq(lines[i]) {
            i += 1;
            continue;
        }
        let name = lines.get(i + 1).unwrap_or(&"").to_string();
        assert!(
            lines.get(i + 2).map(|l| is_eq(l)).unwrap_or(false),
            "corpus header fence missing after '{name}'"
        );
        i += 3;
        let mut input = String::new();
        while i < lines.len() && !is_dash(lines[i]) {
            input.push_str(lines[i]);
            input.push('\n');
            i += 1;
        }
        assert!(i < lines.len(), "corpus separator missing in '{name}'");
        i += 1; // the --- fence
        let mut expected = String::new();
        while i < lines.len() && !is_eq(lines[i]) {
            expected.push_str(lines[i]);
            expected.push('\n');
            i += 1;
        }
        out.push(Case {
            name,
            input,
            expected,
        });
    }
    out
}

/// A scratch path under the system temp directory, removed when the
/// guard is dropped.
///
/// Cleanup written as the last statement of a test runs only when every
/// assertion before it held, so a failing test left its directory
/// behind. `Drop` also runs while a panic unwinds, so this removes the
/// path whether the test passes, fails an assertion or panics.
///
/// The tests of one binary are threads of one process, so `name` must
/// be unique within the binary; the process id is appended. Nothing is
/// created: the path is reserved, and anything stale left at it is
/// removed. Hold the guard for as long as the path is used.
///
/// Two live guards on one path panic. A helper that clears "its" path
/// while a test still holds the same one deletes the test's fixture,
/// and two tests running in parallel on one path clobber each other;
/// both happened here, and both passed silently.
pub struct Scratch(std::path::PathBuf);

static LIVE: std::sync::Mutex<std::collections::BTreeSet<std::path::PathBuf>> =
    std::sync::Mutex::new(std::collections::BTreeSet::new());

impl Scratch {
    pub fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let fresh = LIVE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.clone());
        assert!(
            fresh,
            "{} is already held by another live Scratch: give each test and helper its own name",
            path.display()
        );
        remove_path(&path);
        Scratch(path)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        remove_path(&self.0);
        LIVE.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

fn remove_path(path: &std::path::Path) {
    if std::fs::remove_dir_all(path).is_err() {
        let _ = std::fs::remove_file(path);
    }
}

/// The runtime directory every `kamailio -c` run here is given with
/// `-Y`.
///
/// A configuration check creates nothing in it. Each of the 89
/// configurations the pinned 6.1.5 tree ships was checked with `-Y`
/// naming a fresh empty directory (3 pass; the rest stop at a module
/// the proof install does not build), and so was a configuration that
/// loads every module the install does build and passes. The directory
/// was still empty afterwards. The system temp directory itself is
/// therefore passed, and there is nothing to remove. Every `-Y` goes
/// through this one function so that, should a check ever start
/// writing there, one place changes.
pub fn check_rundir() -> std::path::PathBuf {
    std::env::temp_dir()
}
