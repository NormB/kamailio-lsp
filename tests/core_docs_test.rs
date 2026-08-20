//! Core-language docs come from the kamailio-wiki cookbooks
//! (`docs/cookbooks/<ver>/core.md` and `pseudovariables.md`):
//! `##` sections group the items, one `###` heading per item.

use kamailio_lsp::catalog::{parse_core_cookbook_md, parse_pvars_md};

const CORE: &str = r#"# Core Cookbook

Version: Kamailio SIP Server v6.0.x (stable)

## Overview

Intro prose.

### Not an item

This heading is outside every parameter/function section.

## Core Keywords

### method

Keywords are not parameters; must not be collected.

## Core parameters

### advertised_address

It can be an IP address or string and represents the address
advertised in Via header. Path C:\x kept intact.

``` c
advertised_address="kamailio.example.com"
```

### alias

Parameter to set alias hostnames.

## TCP Parameters

### tcp_accept_haproxy

Enable HAProxy protocol support.

## Core Functions

### exit

Stop the execution of the configuration script.
Second line same paragraph.

Second paragraph excluded.

### force_rport()

Force rport handling.

## Command Line Parameters

### --all-errors

Not a cfg parameter; must not be collected.

## Custom Global Parameters

### String Parameters

Not a cfg parameter either.
"#;

const PVARS: &str = r#"# Pseudo-Variables

## Introduction

Prose with a fenced block:

``` c
### not a heading
```

## The list of pseudo-variables

### $$ - Pseudo-variable marker

No word characters; skipped.

### $\_s(format) - Evaluate dynamic format

Escaped underscore in the heading.

### $ru - Request URI

**$ru** - reference to request's URI.

### $avp(id) - AVPs

Value of the AVP.

## $avp(id) - AVPs

Duplicate section at H2 level; must not double the entry.

## HTable Module

### $sht(htable=>key)

Value of the hash table entry.
"#;

#[test]
fn parses_core_params_from_all_parameter_sections() {
    let (params, _) = parse_core_cookbook_md(CORE).expect("parses");
    let names: Vec<&str> = params.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["advertised_address", "alias", "tcp_accept_haproxy"]
    );
    assert!(params[0].doc.starts_with("It can be an IP address"));
    assert!(params[0].doc.contains(r"C:\x"));
    // the section title becomes the detail
    assert_eq!(params[2].detail, "TCP Parameters");
}

#[test]
fn parses_core_functions() {
    let (_, funcs) = parse_core_cookbook_md(CORE).expect("parses");
    let names: Vec<&str> = funcs.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["exit", "force_rport"]);
    assert!(funcs[0].doc.contains("Second line"));
    assert!(!funcs[0].doc.contains("Second paragraph excluded"));
    assert_eq!(funcs[1].detail, "force_rport()");
}

#[test]
fn keywords_and_cli_sections_are_not_collected() {
    let (params, funcs) = parse_core_cookbook_md(CORE).unwrap();
    assert!(!params.iter().any(|i| i.name == "method"));
    assert!(!params.iter().any(|i| i.name.contains("all-errors")));
    assert!(!params.iter().any(|i| i.name.contains("String")));
    assert!(!funcs.iter().any(|i| i.name == "method"));
}

#[test]
fn parses_pvars_with_escapes_args_and_dedup() {
    let vars = parse_pvars_md(PVARS).expect("parses");
    let names: Vec<&str> = vars.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["$_s", "$ru", "$avp", "$sht"]);
    let ru = &vars[1];
    assert_eq!(ru.detail, "Request URI");
    assert!(ru.doc.contains("reference to request's URI"));
    // the argument form survives in the detail of parameterized pvars
    assert_eq!(vars[3].detail, "$sht(htable=>key)");
}

#[test]
fn fenced_headings_are_ignored() {
    let vars = parse_pvars_md(PVARS).unwrap();
    assert!(!vars.iter().any(|v| v.name.contains("not")));
}

#[test]
fn adversarial_core_docs_do_not_panic() {
    for s in [
        "",
        "\0",
        "## Core parameters\n### ",
        "### $",
        "## Core Functions\n### (\n",
        "### $\\\\ - x",
        "## Core parameters\n### p\n   C:\\x\\y\n",
    ] {
        let _ = parse_core_cookbook_md(s);
        let _ = parse_pvars_md(s);
    }
    assert!(parse_core_cookbook_md("").is_err());
    assert!(parse_pvars_md("\0").is_err());
}

#[test]
fn harvests_core_docs_from_a_real_wiki_checkout_when_present() {
    let Ok(wiki) = std::env::var("KAMAILIO_LSP_TEST_WIKI") else {
        eprintln!("SKIP: KAMAILIO_LSP_TEST_WIKI not set");
        return;
    };
    let core = kamailio_lsp::catalog::harvest_core(std::path::Path::new(&wiki));
    assert!(
        core.functions.len() > 20,
        "core functions: {}",
        core.functions.len()
    );
    assert!(
        core.params.len() > 150,
        "core params: {}",
        core.params.len()
    );
    assert!(core.pvars.len() > 80, "pvars: {}", core.pvars.len());
    assert!(core.functions.iter().any(|f| f.name == "exit"));
    assert!(core.params.iter().any(|p| p.name == "advertised_address"));
    assert!(
        core.params
            .iter()
            .any(|p| p.name == "tcp_connection_lifetime")
    );
    assert!(core.pvars.iter().any(|v| v.name == "$ru"));
    assert!(core.pvars.iter().any(|v| v.name == "$avp"));
}
