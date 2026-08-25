//! `kamailio-lsp check` — the analyzer (plus the real `kamailio -c`
//! when a binary is configured) as a CLI for CI pipelines and git
//! hooks.  Exit codes: 0 clean (warnings allowed), 1 findings
//! (errors, or warnings under `--strict`), 2 usage/read failures.

use crate::{diag, logic};

fn disk_loader(p: &std::path::Path) -> Option<String> {
    let md = std::fs::metadata(p).ok()?;
    if !md.is_file() || md.len() > 1_048_576 {
        return None;
    }
    std::fs::read_to_string(p).ok()
}

/// Run the `check` subcommand over `args` (everything after `check`).
/// Prints findings as `file:line:col: severity: message` (1-based).
pub fn run_check(args: &[String]) -> i32 {
    let mut strict = false;
    let mut bin_flag: Option<String> = None;
    let mut files: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--strict" => strict = true,
            "--bin" => match it.next() {
                Some(b) => bin_flag = Some(b.clone()),
                None => {
                    eprintln!("--bin needs a path");
                    return 2;
                }
            },
            _ if a.starts_with("--") => {
                eprintln!("unknown flag {a}");
                return 2;
            }
            _ => files.push(a.clone()),
        }
    }
    if files.is_empty() {
        eprintln!("usage: kamailio-lsp check [--strict] [--bin <kamailio>] <file>...");
        return 2;
    }
    let bin = logic::resolve_bin(bin_flag.as_deref(), std::env::var("KAMAILIO_LSP_BIN").ok());

    let mut errors = 0usize;
    let mut warnings = 0usize;
    for f in &files {
        let path = std::path::Path::new(f);
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("{f}: cannot read");
            return 2;
        };
        let mut findings: Vec<(u32, u32, bool, String)> = Vec::new();
        // fast analyzer pass (warnings)
        for d in logic::analyzer_diagnostics(path, &text, &disk_loader) {
            findings.push((d.line, d.col_start, false, d.message));
        }
        // the real parser, when configured: the same invocation the
        // server uses (-c needs a writable runtime dir even to check)
        if let Some(bin) = &bin {
            let out = std::process::Command::new(bin)
                .args(["-c", "--all-errors", "-Y"])
                .arg(std::env::temp_dir())
                .arg("-f")
                .arg(path)
                .current_dir(path.parent().unwrap_or(std::path::Path::new(".")))
                .output();
            match out {
                Ok(out) => {
                    let rc = out.status.code().unwrap_or(-1);
                    let all = format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    let parsed = diag::parse_check_output(&all, rc);
                    let mut attached = 0usize;
                    for d in &parsed {
                        let is_err = d.severity == diag::Severity::Error;
                        if let Some(m) = logic::remap_include_diag(path, &text, d) {
                            findings.push((m.line, m.col_start, is_err, m.message));
                            attached += 1;
                        }
                    }
                    if attached == 0 && rc != 0 {
                        // everything pointed elsewhere: never silent
                        findings.push((0, 0, true, logic::check_failure_note(parsed.first(), rc)));
                    }
                }
                Err(e) => {
                    eprintln!("{f}: cannot run '{bin} -c': {e}");
                    return 2;
                }
            }
        }
        findings.sort_by_key(|(l, c, _, _)| (*l, *c));
        for (line, col, is_err, msg) in findings {
            let sev = if is_err { "error" } else { "warning" };
            println!("{f}:{}:{}: {sev}: {msg}", line + 1, col + 1);
            if is_err {
                errors += 1;
            } else {
                warnings += 1;
            }
        }
    }
    if errors > 0 || (strict && warnings > 0) {
        1
    } else {
        0
    }
}
