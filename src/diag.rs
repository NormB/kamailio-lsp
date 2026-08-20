//! `kamailio -c` execution and output parsing.

/// Diagnostic severity, mirroring the LSP levels we emit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    /// A parse or load error: the cfg will not start.
    Error,
    /// A non-fatal complaint.
    Warning,
}

/// One diagnostic parsed out of `kamailio -c` output.
#[derive(Debug, Clone, PartialEq)]
pub struct Diag {
    /// File the parser attributed the error to (may be empty for
    /// the global fallback diagnostic).
    pub file: String,
    /// 0-based.
    pub line: u32,
    /// 0-based end line (== `line` except for spanning errors).
    pub end_line: u32,
    /// 0-based start column.
    pub col_start: u32,
    /// Exclusive end column.
    pub col_end: u32,
    /// Mapped severity.
    pub severity: Severity,
    /// Human-readable message (the yyerror_at tail).
    pub message: String,
}

/// Parse the stderr of `kamailio -c [--all-errors] -f <file>` into
/// diagnostics.
///
/// Positioned lines come from `yyerror_at()`/`warn_at()` in
/// `src/core/cfg.y` and carry one of three shapes after the
/// `parse error in config file ` / `warning in config file ` prefix
/// (all positions 1-based):
///
/// - `<path>, line N, column C: <msg>`
/// - `<path>, line N, column C1-C2: <msg>` (C2 inclusive)
/// - `<path>, from line N, column C to line M, column D: <msg>`
///
/// Everything else is noise, except as a fallback when the check
/// failed without any positioned error.
pub fn parse_check_output(output: &str, rc: i32) -> Vec<Diag> {
    // one-line variants: `line N, column C[-C2]: msg`
    let single = regex::Regex::new(
        r"(CRITICAL|ERROR|WARNING): .*(parse error|warning) in config file (.*?), line (\d+), column (\d+)(?:-(\d+))?: ?(.*)",
    )
    .unwrap();
    // spanning variant: `from line N, column C to line M, column D: msg`
    let spanning = regex::Regex::new(
        r"(CRITICAL|ERROR|WARNING): .*(parse error|warning) in config file (.*?), from line (\d+), column (\d+) to line (\d+), column (\d+): ?(.*)",
    )
    .unwrap();
    let mut out = Vec::new();

    for text_line in output.lines() {
        // try the spanning shape first: the single-line regex would
        // otherwise mis-split its `from line ...` tail
        let (file, s_line, s_col, e_line, e_col, sev_word, msg);
        if let Some(c) = spanning.captures(text_line) {
            let (Ok(sl), Ok(sc), Ok(el), Ok(ec)) = (
                c[4].parse::<u32>(),
                c[5].parse::<u32>(),
                c[6].parse::<u32>(),
                c[7].parse::<u32>(),
            ) else {
                continue; // absurd numbers: skip rather than lie
            };
            file = c[3].to_string();
            sev_word = if &c[2] == "warning" {
                Severity::Warning
            } else {
                Severity::Error
            };
            (s_line, s_col, e_line, e_col) = (sl, sc, el, ec);
            msg = truncate_message(c[8].trim());
        } else if let Some(c) = single.captures(text_line) {
            let Ok(sl) = c[4].parse::<u32>() else {
                continue;
            };
            let Ok(sc) = c[5].parse::<u32>() else {
                continue;
            };
            let ec = match c.get(6) {
                Some(m) => match m.as_str().parse::<u32>() {
                    Ok(v) => v,
                    Err(_) => continue,
                },
                None => sc, // single-column shape
            };
            file = c[3].to_string();
            sev_word = if &c[2] == "warning" {
                Severity::Warning
            } else {
                Severity::Error
            };
            (s_line, s_col, e_line, e_col) = (sl, sc, sl, ec);
            msg = truncate_message(c[7].trim());
        } else {
            continue;
        }
        let line = s_line.saturating_sub(1);
        let end_line = e_line.saturating_sub(1).max(line);
        let col_start = s_col.saturating_sub(1);
        // KAMAILIO's C2 is INCLUSIVE (verified live: `blah` at bytes
        // 6..10 reports `column 7-10`), unlike OpenSIPS's exclusive
        // end — inclusive 1-based end == exclusive 0-based end, so no
        // +-1 here; never narrower than one column, never reversed
        let col_end = if end_line == line {
            e_col.max(s_col).max(1)
        } else {
            e_col.max(1)
        };
        out.push(Diag {
            file,
            line,
            end_line,
            col_start,
            col_end,
            severity: sev_word,
            message: if msg.is_empty() {
                "parse error".to_string()
            } else {
                msg
            },
        });
    }

    if out.is_empty() && rc != 0 {
        // truly unpositioned failures only (a missing module still
        // yields a positioned "failed to load module" line); e.g.
        // ` 0(99) ERROR: <core> [main.c:3141]: main(): failed to
        // create runtime dir /var/run/kamailio/, ...`
        let generic =
            regex::Regex::new(r"(?:ERROR|CRITICAL): \S+ \[[^\]]*\]: [A-Za-z0-9_]+\(\): (.+)")
                .unwrap();
        let msg = output
            .lines()
            .rev()
            .find_map(|l| generic.captures(l).map(|c| truncate_message(c[1].trim())))
            .unwrap_or_else(|| format!("kamailio -c failed (rc={rc}) with unparseable output"));
        out.push(Diag {
            file: String::new(),
            line: 0,
            end_line: 0,
            col_start: 0,
            col_end: 1,
            severity: Severity::Error,
            message: msg,
        });
    }
    out
}

/// Editors render diagnostics inline: bound the message so a hostile
/// or broken checker cannot ship megabytes into the client.
const MESSAGE_CAP: usize = 500;

fn truncate_message(msg: &str) -> String {
    if msg.len() <= MESSAGE_CAP {
        return msg.to_string();
    }
    let mut end = MESSAGE_CAP;
    while end > 0 && !msg.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\u{2026}", &msg[..end])
}
