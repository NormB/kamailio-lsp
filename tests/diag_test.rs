use kamailio_lsp::diag::{Severity, parse_check_output};

// Verbatim `kamailio -c -Y /tmp -f bad.cfg` stderr captured from the
// real 6.0.1 binary (2026-08-19); single-column variant.
const REAL_SINGLE: &str = r#" 0(3000286) CRITICAL: <core> [core/cfg.y:4048]: yyerror_at(): parse error in config file /tmp/kamtest/bad.cfg, line 4, column 1: syntax error
"#;

// Verbatim `kamailio -c --all-errors` stderr (2026-08-19): the
// column-range variant, multiple errors in one run.
const REAL_MULTI: &str = r#" 0(3000288) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 3, column 7-10: syntax error
 0(3000288) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 3, column 7-10: '('')' expected (function call)
 0(3000288) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 3, column 7-10: bad command: missing ';'?
 0(3000288) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 4, column 6-8: '('')' expected (function call)
 0(3000288) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 4, column 6-8: bad command: missing ';'?
"#;

// Verbatim capture of a modparam failure: a module ERROR line without
// positions, then the positioned CRITICAL (2026-08-19).
const REAL_MODPARAM: &str = r#" 0(3000287) ERROR: <core> [core/modparam.c:187]: set_mod_param_regex(): parameter <nosuchparam> of type <2:int> not found in module <tm>
 0(3000287) CRITICAL: <core> [core/cfg.y:4048]: yyerror_at(): parse error in config file /tmp/kamtest/modp.cfg, line 3, column 30: Can't set module parameter
"#;

// The spanning variant, per the exact format string in
// src/core/cfg.y (yyerror_at, e_line != s_line branch).
const SPANNING: &str = r#" 0(99) CRITICAL: <core> [core/cfg.y:4045]: yyerror_at(): parse error in config file /tmp/kamtest/span.cfg, from line 3, column 5 to line 4, column 2: syntax error
"#;

// The warning shape, per the warn_at format strings in cfg.y (all
// three positional variants share the "warning in config file" tail).
const WARNING: &str = r#" 0(99) WARNING: <core> [core/cfg.y:4030]: warn_at(): warning in config file /tmp/kamtest/w.cfg, line 2, column 3-4: tcp support not compiled in
"#;

// Verbatim captures from the real 6.1.4 binary (2026-08-23) — the
// current stable line.  6.1 emits from a different line of cfg.y, so
// the `[core/cfg.y:NNNN]` tag differs from the 6.0.1 captures above;
// that tag is log noise the parser must go on ignoring.  Every
// positional shape is unchanged between the two release lines.
const REAL_61_SINGLE: &str = r#" 0(2281906) CRITICAL: <core> [core/cfg.y:4125]: yyerror_at(): parse error in config file /tmp/kamtest/bad.cfg, line 5, column 1: syntax error
 0(2281906) CRITICAL: <core> [core/cfg.y:4125]: yyerror_at(): parse error in config file /tmp/kamtest/bad.cfg, line 5, column 1: 
"#;

const REAL_61_MULTI: &str = r#" 0(2281911) CRITICAL: <core> [core/cfg.y:4122]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 3, column 9-11: syntax error
 0(2281911) CRITICAL: <core> [core/cfg.y:4122]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 3, column 9-11: '('')' expected (function call)
 0(2281911) CRITICAL: <core> [core/cfg.y:4122]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 3, column 9-11: bad command: missing ';'?
 0(2281911) CRITICAL: <core> [core/cfg.y:4122]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 4, column 9-11: '('')' expected (function call)
 0(2281911) CRITICAL: <core> [core/cfg.y:4122]: yyerror_at(): parse error in config file /tmp/kamtest/multi.cfg, line 4, column 9-11: bad command: missing ';'?
"#;

const REAL_61_MODPARAM: &str = r#" 0(2281916) ERROR: <core> [core/modparam.c:217]: set_mod_param_regex(): parameter <nosuchparam> of type <2:int> not found in module <tm>
 0(2281916) CRITICAL: <core> [core/cfg.y:4125]: yyerror_at(): parse error in config file /tmp/kamtest/modp.cfg, line 3, column 32: Can't set module parameter
"#;

#[test]
fn parses_single_column_variant() {
    let ds = parse_check_output(REAL_SINGLE, 255);
    assert_eq!(ds.len(), 1);
    let d = &ds[0];
    assert_eq!(d.file, "/tmp/kamtest/bad.cfg");
    assert_eq!(d.line, 3); // 1-based 4 -> 0-based 3
    assert_eq!(d.end_line, 3);
    assert_eq!(d.col_start, 0); // 1-based 1 -> 0
    assert_eq!(d.col_end, 1); // at least one column wide
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.message, "syntax error");
    assert!(!d.message.contains("yyerror")); // internal tag stripped
}

#[test]
fn parses_column_range_variant_and_all_errors_runs() {
    let ds = parse_check_output(REAL_MULTI, 255);
    assert_eq!(ds.len(), 5, "--all-errors yields one diag per line");
    let d = &ds[0];
    assert_eq!(d.line, 2);
    assert_eq!(d.col_start, 6); // 1-based 7 -> 6
    assert_eq!(d.col_end, 10); // inclusive 1-based 10 == exclusive 0-based 10
    assert_eq!(ds[3].line, 3);
    assert!(ds[4].message.contains("missing ';'"));
}

#[test]
fn modparam_failure_uses_the_positioned_line() {
    let ds = parse_check_output(REAL_MODPARAM, 255);
    assert_eq!(ds.len(), 1, "unpositioned ERROR noise is not a diag");
    assert_eq!(ds[0].line, 2);
    assert_eq!(ds[0].col_start, 29);
    assert_eq!(ds[0].message, "Can't set module parameter");
}

#[test]
fn parses_the_spanning_variant() {
    let ds = parse_check_output(SPANNING, 255);
    assert_eq!(ds.len(), 1);
    let d = &ds[0];
    assert_eq!(d.line, 2); // from line 3
    assert_eq!(d.col_start, 4); // from column 5
    assert_eq!(d.end_line, 3); // to line 4
    assert_eq!(d.col_end, 2); // to column 2
    assert_eq!(d.message, "syntax error");
}

#[test]
fn parses_the_warning_shape_as_warning() {
    let ds = parse_check_output(WARNING, 0);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].severity, Severity::Warning);
    assert_eq!(ds[0].line, 1);
    assert_eq!(ds[0].col_start, 2);
    assert_eq!(ds[0].col_end, 4);
    assert!(ds[0].message.contains("tcp support"));
}

#[test]
fn clean_output_zero_rc_is_empty() {
    let out = "config file ok, exiting...\nListening on\n             udp: 127.0.0.1:5060\n";
    assert!(parse_check_output(out, 0).is_empty());
}

#[test]
fn nonzero_rc_with_no_positioned_error_yields_fallback() {
    // a missing module is NOT this case (it emits a positioned
    // "failed to load module" line); the true unpositioned failure is
    // e.g. a missing runtime dir — verbatim capture of `kamailio -c`
    // without -Y (2026-08-20)
    let out = " 0(99) ERROR: <core> [main.c:3141]: main(): failed to create runtime dir /var/run/kamailio/, check directory permissions\n";
    let ds = parse_check_output(out, 255);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].line, 0);
    assert_eq!(ds[0].end_line, 0);
    assert!(
        ds[0].message.contains("failed to create runtime dir"),
        "fallback message: {}",
        ds[0].message
    );
}

#[test]
fn nonzero_rc_with_unparseable_output_still_yields_a_diag() {
    let ds = parse_check_output("total garbage\n", 255);
    assert_eq!(ds.len(), 1);
    assert!(ds[0].message.contains("rc=255"));
}

#[test]
fn adversarial_output_does_not_panic() {
    for s in [
        "",
        "\0\0",
        "CRITICAL: <core> [x]: yyerror_at(): parse error in config file x.cfg, line 99999999999999999999, column 5-6: overflow",
        "CRITICAL: <core> [x]: yyerror_at(): parse error in config file , line 0, column 0-0:",
        "parse error in config file a.cfg, line 3, column 9-8: reversed",
        "parse error in config file a, line b.cfg, line 3, column 1: comma path",
        "no colons at all",
        "parse error in config file x.cfg, from line 2, column 9 to line 1, column 1: reversed lines",
        "CRITICAL: \\ [\\]: \\(): parse error in config file C:\\x\\y.cfg, line 1, column 1: backslashes",
    ] {
        let _ = parse_check_output(s, 1);
    }
}

#[test]
fn column_range_never_reversed() {
    let ds = parse_check_output(
        " 0(1) CRITICAL: <core> [core/cfg.y:1]: yyerror_at(): parse error in config file a.cfg, line 3, column 9-8: reversed\n",
        1,
    );
    if let Some(d) = ds.first() {
        assert!(d.col_end >= d.col_start);
    }
}

#[test]
fn spanning_end_line_never_precedes_start_line() {
    let ds = parse_check_output(
        " 0(1) CRITICAL: <core> [core/cfg.y:1]: yyerror_at(): parse error in config file a.cfg, from line 5, column 2 to line 3, column 1: reversed\n",
        1,
    );
    if let Some(d) = ds.first() {
        assert!(d.end_line >= d.line);
    }
}

#[test]
fn absurdly_long_messages_are_truncated() {
    let long = "x".repeat(50_000);
    let out = format!(
        " 0(1) CRITICAL: <core> [core/cfg.y:1]: yyerror_at(): parse error in config file a.cfg, line 1, column 1: {long}\n"
    );
    let ds = parse_check_output(&out, 255);
    assert_eq!(ds.len(), 1);
    assert!(
        ds[0].message.len() <= 600,
        "message must be bounded, got {} bytes",
        ds[0].message.len()
    );
    assert!(ds[0].message.ends_with('…'), "truncation must be visible");
}

// Kamailio 5.x prints the same yyerror_at() format strings (they
// predate 6.x); only the log prefix differs — older trees log the
// grammar file without the core/ dir and builds may carry timestamps.
// The parser must treat both generations identically.
const FIVE_X: &str = r#"Aug 20 05:01:02 kamailio 0(29) CRITICAL: <core> [cfg.y:3423]: yyerror_at(): parse error in config file /tmp/kamtest/five.cfg, line 12, column 4-7: syntax error
 0(29) CRITICAL: <core> [cfg.y:3423]: yyerror_at(): parse error in config file /tmp/kamtest/five.cfg, line 13, column 2: unknown command, missing loadmodule?
"#;

#[test]
fn parses_5x_style_output_identically() {
    let ds = parse_check_output(FIVE_X, 255);
    assert_eq!(ds.len(), 2);
    assert_eq!(ds[0].line, 11);
    assert_eq!(ds[0].col_start, 3);
    assert_eq!(ds[0].col_end, 7);
    assert_eq!(ds[1].line, 12);
    assert!(ds[1].message.contains("unknown command"));
}

#[test]
fn the_6_1_shapes_parse_exactly_as_the_6_0_ones_do() {
    // single-column, plus the empty-message trailer 6.1 emits after it
    let ds = parse_check_output(REAL_61_SINGLE, 255);
    assert_eq!(ds.len(), 2);
    assert_eq!(ds[0].file, "/tmp/kamtest/bad.cfg");
    assert_eq!(ds[0].line, 4); // 1-based 5 -> 0-based 4
    assert_eq!(ds[0].col_start, 0);
    assert_eq!(ds[0].col_end, 1);
    assert_eq!(ds[0].severity, Severity::Error);
    assert_eq!(ds[0].message, "syntax error");
    // an empty tail message still carries a real position: it must
    // never reach the editor blank
    assert_eq!(ds[1].message, "parse error");

    // column-range, one diag per --all-errors line
    let ds = parse_check_output(REAL_61_MULTI, 255);
    assert_eq!(ds.len(), 5);
    assert_eq!(ds[0].line, 2);
    assert_eq!(ds[0].col_start, 8); // 1-based 9 -> 8
    assert_eq!(ds[0].col_end, 11); // inclusive 1-based 11 == exclusive 0-based 11
    assert_eq!(ds[3].line, 3);
    assert!(ds[4].message.contains("missing ';'"));

    // modparam: the unpositioned ERROR stays noise, the CRITICAL is the diag
    let ds = parse_check_output(REAL_61_MODPARAM, 255);
    assert_eq!(ds.len(), 1, "unpositioned ERROR noise is not a diag");
    assert_eq!(ds[0].line, 2);
    assert_eq!(ds[0].col_start, 31);
    assert_eq!(ds[0].message, "Can't set module parameter");
}
