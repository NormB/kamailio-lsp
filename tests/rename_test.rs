//! What rename, references and highlights treat as an occurrence of a
//! route name, and when a rename must be refused.
//!
//! Ground truth, Kamailio 6.1.4:
//!
//! - `t_on_failure`, `t_on_reply` and `t_on_branch` take one string
//!   that `fixup_routes` looks up in the failure, onreply or branch
//!   route list (`src/modules/tm/tm.c`).  `kamailio -c` accepts the
//!   name double- or single-quoted and rejects it bare (`parameter 1
//!   is not constant`).
//! - Two definitions of one route name in one table are rejected:
//!   `-c` reports `duplicate route` for `route[X]` twice and for
//!   `failure_route[X]` twice.

use kamailio_lsp::logic::{RouteNs, ns_occurrences, rename_conflict, route_symbol_ns_at};
use std::path::PathBuf;

const ARMED: &str = r#"request_route {
    t_on_failure("FR");
    t_on_failure('FR');
    t_on_failure("$var(r)");
    xlog("FR");
    # t_on_failure("FR");
    t_on_reply("FR");
    t_on_branch("BR");
    t_relay();
}
failure_route[FR] {
    exit;
}
onreply_route[FR] {
    exit;
}
branch_route[BR] {
    exit;
}
"#;

fn spots(occ: &[(kamailio_lsp::analyze::Located, bool)]) -> Vec<(u32, u32, bool)> {
    let mut v: Vec<_> = occ.iter().map(|(l, d)| (l.line, l.col, *d)).collect();
    v.sort();
    v
}

#[test]
fn a_failure_route_is_named_by_the_strings_that_arm_it() {
    let occ = ns_occurrences(ARMED, "FR", &RouteNs::Kind("failure_route".into()));
    assert_eq!(
        spots(&occ),
        vec![(1, 18, false), (2, 18, false), (10, 14, true)],
        "a pvar string, an xlog string, a comment and t_on_reply's \
         \"FR\" are not failure-route references"
    );
}

#[test]
fn reply_and_branch_routes_are_armed_by_their_own_functions() {
    let reply = ns_occurrences(ARMED, "FR", &RouteNs::Kind("onreply_route".into()));
    assert_eq!(spots(&reply), vec![(6, 16, false), (13, 14, true)]);
    let branch = ns_occurrences(ARMED, "BR", &RouteNs::Kind("branch_route".into()));
    assert_eq!(spots(&branch), vec![(7, 17, false), (16, 13, true)]);
}

#[test]
fn the_cursor_on_an_arming_string_names_that_routes_namespace() {
    assert_eq!(
        route_symbol_ns_at(ARMED, 1, 19),
        Some(("FR".into(), RouteNs::Kind("failure_route".into())))
    );
    assert_eq!(
        route_symbol_ns_at(ARMED, 6, 17),
        Some(("FR".into(), RouteNs::Kind("onreply_route".into())))
    );
    assert_eq!(route_symbol_ns_at(ARMED, 3, 20), None);
}

#[test]
fn a_rename_onto_an_existing_route_in_the_same_namespace_conflicts() {
    let files = vec![
        (
            PathBuf::from("/a.cfg"),
            "route[TAKEN] {\n    exit;\n}\n".to_string(),
        ),
        (
            PathBuf::from("/b.cfg"),
            "route[OTHER] {\n    exit;\n}\nfailure_route[FREE] {\n    exit;\n}\n".to_string(),
        ),
    ];
    let hit = rename_conflict(&files, "TAKEN", &RouteNs::Main).expect("TAKEN is defined");
    assert_eq!(hit.0, PathBuf::from("/a.cfg"));
    assert_eq!((hit.1.line, hit.1.col), (0, 6));
    assert!(rename_conflict(&files, "FREE", &RouteNs::Main).is_none());
    assert!(rename_conflict(&files, "TAKEN", &RouteNs::Kind("failure_route".into())).is_none());
    assert!(rename_conflict(&files, "FREE", &RouteNs::Kind("failure_route".into())).is_some());
    let called = vec![(
        PathBuf::from("/c.cfg"),
        "request_route {\n    route(GHOST);\n}\n".to_string(),
    )];
    assert!(rename_conflict(&called, "GHOST", &RouteNs::Main).is_none());
}
