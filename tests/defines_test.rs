//! `#!define` and friends: the preprocessor symbols the server has
//! been blind to.
//!
//! Every directive here is one kamailio 6.1's `src/core/cfg.lex`
//! actually recognises (`PREP_START` is `#!` or `!!`), and the
//! route-target behaviour is what the 6.1.4 binary accepts.

mod common;
use kamailio_lsp::analyze::defines;

#[test]
fn every_defining_directive_form_is_collected() {
    // cfg.lex: define|def, trydefine|trydef, redefine|redef, defexp,
    // defexps, defenv, defenvs, trydefenv, trydefenvs — behind either
    // prefix
    let text = "\
#!define PLAIN
#!define WITH_VALUE 42
#!def SHORT 1
!!define BANG_PREFIX 2
#!trydefine TRY 3
#!trydef TRY_SHORT 4
#!redefine RE 5
#!redef RE_SHORT 6
#!defexp EXP 1+1
#!defenv FROM_ENV
#!trydefenvs TRY_ENV_S
request_route { exit; }
";
    let got: Vec<(String, String)> = defines(text)
        .into_iter()
        .map(|d| (d.name, d.value))
        .collect();
    assert_eq!(
        got,
        vec![
            ("PLAIN".into(), String::new()),
            ("WITH_VALUE".into(), "42".into()),
            ("SHORT".into(), "1".into()),
            ("BANG_PREFIX".into(), "2".into()),
            ("TRY".into(), "3".into()),
            ("TRY_SHORT".into(), "4".into()),
            ("RE".into(), "5".into()),
            ("RE_SHORT".into(), "6".into()),
            ("EXP".into(), "1+1".into()),
            ("FROM_ENV".into(), String::new()),
            ("TRY_ENV_S".into(), String::new()),
        ]
    );
}

#[test]
fn a_backslash_continued_define_keeps_its_whole_value() {
    let text = "#!define LONG one \\\n    two \\\n    three\nrequest_route { exit; }\n";
    let d = defines(text);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].name, "LONG");
    assert!(
        d[0].value.contains("one") && d[0].value.contains("three"),
        "continuation lost: {:?}",
        d[0].value
    );
}

#[test]
fn substdef_binds_a_name_too() {
    // `#!substdef "!NAME!value!g"` — the first character after the
    // quote is the delimiter, whatever it is
    let text = "#!substdef \"!MYPORT!5060!g\"\n#!substdef \"/OTHER/x/\"\nrequest_route { exit; }\n";
    let got: Vec<(String, String)> = defines(text)
        .into_iter()
        .map(|d| (d.name, d.value))
        .collect();
    assert_eq!(
        got,
        vec![
            ("MYPORT".into(), "5060".into()),
            ("OTHER".into(), "x".into()),
        ]
    );
}

#[test]
fn positions_point_at_the_name() {
    let text = "#!KAMAILIO\n#!define RELAY 1\nrequest_route { exit; }\n";
    let d = defines(text);
    assert_eq!(d.len(), 1, "the shebang is not a define: {d:?}");
    assert_eq!(d[0].name, "RELAY");
    assert_eq!(d[0].line, 1);
    assert_eq!(d[0].col, 9, "column of the NAME, not the directive");
}

#[test]
fn conditionals_and_substs_are_not_definitions() {
    let text = "\
#!ifdef WITH_TLS
#!ifndef WITH_TLS
#!else
#!endif
#!subst \"/a/b/\"
request_route { exit; }
";
    assert!(defines(text).is_empty(), "{:?}", defines(text));
}

#[test]
fn a_define_inside_a_string_or_comment_is_not_a_define() {
    let text =
        "request_route {\n    $var(x) = \"#!define FAKE 1\";\n    # #!define ALSO_FAKE 2\n}\n";
    assert!(defines(text).is_empty(), "{:?}", defines(text));
}

#[test]
fn adversarial_input_does_not_panic() {
    for s in [
        "#!define",
        "#!define ",
        "#!substdef",
        "#!substdef \"\"",
        "#!substdef \"!only\"",
        "!!",
        "#!define \u{1F600} 1",
        "#!define A \\",
    ] {
        let _ = defines(s);
    }
}
