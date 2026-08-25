//! The C parameter table is the list `modparam()` is checked against
//! when Kamailio starts, so it decides which parameters exist. These
//! prove the extraction against the shapes the 6.1.4 tree actually
//! uses: five declaration spellings, tables assembled entirely from
//! macros, conditional entries, and the terminator and comment forms
//! that must not be read as names.

use kamailio_lsp::catalog::parse_param_export_tables;

#[test]
fn reads_the_params_spelling() {
    let src = r#"
static param_export_t params[] = {
		{"node_hostname", PARAM_STR, &dbk_node_hostname},
		{"amqp_consumer_ack_timeout_micro", PARAM_INT, &kz_ack_tv.tv_usec},
		{0, 0, 0}
};
"#;
    assert_eq!(
        parse_param_export_tables(src),
        vec!["node_hostname", "amqp_consumer_ack_timeout_micro"]
    );
}

/// Kamailio writes these five, none of them `const`.
#[test]
fn reads_every_identifier_spelling() {
    for ident in [
        "params",
        "mod_params",
        "parameters",
        "mod_parms",
        "cdp_params",
    ] {
        let src = format!(
            "static param_export_t {ident}[] = {{\n\t{{\"only\", PARAM_INT, &x}},\n\t{{0,0,0}}\n}};\n"
        );
        assert_eq!(
            parse_param_export_tables(&src),
            vec!["only"],
            "identifier {ident} must be read"
        );
    }
}

#[test]
fn unions_several_tables_in_one_file() {
    let src = r#"
static param_export_t params[] = {
	{"first", PARAM_INT, &a},
	{0, 0, 0}
};
static param_export_t mod_params[] = {
	{"second", PARAM_INT, &b},
	{0, 0, 0}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["first", "second"]);
}

#[test]
fn terminators_are_not_parameters() {
    let src = r#"
static param_export_t params[] = {
	{"real", PARAM_INT, &a},
	{NULL, 0, NULL}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["real"]);
}

#[test]
fn commented_out_entries_are_not_parameters() {
    let src = r#"
static param_export_t params[] = {
	{"live", PARAM_INT, &a},
	/* {"block_commented", PARAM_INT, &b}, */
	// {"line_commented", PARAM_INT, &c},
	{0, 0, 0}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["live"]);
}

/// `cmd_export_t` names are script functions, a different namespace.
#[test]
fn other_export_tables_are_ignored() {
    let src = r#"
static cmd_export_t cmds[] = {
	{"matrix", (cmd_function)w_matrix, 3, fixup, 0, REQUEST_ROUTE},
	{0, 0, 0, 0, 0, 0}
};
static param_export_t params[] = {
	{"the_only_param", PARAM_INT, &a},
	{0, 0, 0}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["the_only_param"]);
}

#[test]
fn a_bare_type_mention_opens_no_table() {
    let src = r#"
int register_params(param_export_t *p);
static param_export_t params[] = {
	{"actual", PARAM_INT, &a},
	{0, 0, 0}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["actual"]);
}

#[test]
fn a_file_with_no_table_yields_nothing() {
    assert!(parse_param_export_tables("int main(void) { return 0; }").is_empty());
}

/// Only the literal that opens an entry is a name; one nested deeper
/// inside the entry's value is an argument. The nesting here is a
/// brace opening directly on a literal, because a fixture that nests
/// in parentheses — or in a brace whose first token is a designator —
/// never reaches the depth the guard tests and would pass whether the
/// guard were right or wrong.
#[test]
fn only_the_leading_literal_of_an_entry_is_a_name() {
    let src = r#"
static param_export_t params[] = {
	{"paren_arg", PARAM_STR, fixup("not_a_param")},
	{"brace_arg", PARAM_STR, &(struct opt){ "also_not_a_param", 0 }},
	{0, 0, 0}
};
"#;
    assert_eq!(
        parse_param_export_tables(src),
        vec!["paren_arg", "brace_arg"]
    );
}

/// `#ifdef` brackets entries conditionally — `tm` guards one behind
/// `USE_DNS_FAILOVER` and `tls` behind `KSR_SSL_ENGINE`. A catalogue
/// wants the union of both arms, and the directive itself must not be
/// mistaken for a macro that splices entries in.
#[test]
fn conditional_entries_are_all_collected() {
    let src = r#"
static param_export_t params[] = {
	{"always", PARAM_INT, &a},
#ifdef USE_DNS_FAILOVER
	{"only_with_failover", PARAM_INT, &b},
#endif
	{"also_always", PARAM_INT, &c},
	{0, 0, 0}
};
"#;
    assert_eq!(
        parse_param_export_tables(src),
        vec!["always", "only_with_failover", "also_always"]
    );
}
