//! The function catalogue against the C sources of every pinned
//! Kamailio release.
//!
//! The catalogue is harvested from the module READMEs. These gates
//! check it from the other side: every function it lists is a name the
//! module's C sources contain as a string, and the aliases it adds are
//! exactly the ones the C export tables define.

mod common;

/// (release, module, function, why) for each catalogue function the C
/// sources never name, because the upstream README documents a name
/// the module does not export. Each must keep firing: an entry that
/// stops firing is removed, so the list cannot hide a regression.
// Measured against 5.8.8, 6.0.8 and 6.1.5: each README heading names a
// function its module's C sources never contain as a string literal.
const UPSTREAM_DOC_ERRORS: &[(&str, &str, &str, &str)] = &[
    (
        "5.8.8",
        "avpops",
        "avp_printf",
        "documented; avpops exports avp_print, and avp_printf is kex (kex_mod.c)",
    ),
    (
        "5.8.8",
        "cnxcc",
        "cnxcc_set_max_channel",
        "heading drops the final s; cnxcc exports cnxcc_set_max_channels",
    ),
    (
        "5.8.8",
        "db2_ops",
        "db_close",
        "exported as MODULE_NAME2 \"_close\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "5.8.8",
        "db2_ops",
        "db_first",
        "exported as MODULE_NAME2 \"_first\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "5.8.8",
        "db2_ops",
        "db_foreach",
        "exported as MODULE_NAME2 \"_foreach\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "5.8.8",
        "db2_ops",
        "db_next",
        "exported as MODULE_NAME2 \"_next\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "5.8.8",
        "db2_ops",
        "db_proper",
        "exported as MODULE_NAME2 \"_proper\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "5.8.8",
        "db2_ops",
        "db_query",
        "exported as MODULE_NAME2 \"_query\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "5.8.8",
        "db2_ops",
        "db_seek",
        "exported as MODULE_NAME2 \"_seek\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "5.8.8",
        "ims_auth",
        "ims_www_authorize",
        "documented; ims_auth exports ims_www_authenticate",
    ),
    (
        "5.8.8",
        "ims_dialog",
        "dlg_bridge",
        "documented; ims_dialog exports no dlg_bridge",
    ),
    (
        "5.8.8",
        "ims_dialog",
        "dlg_manage",
        "documented; ims_dialog exports no dlg_manage",
    ),
    (
        "5.8.8",
        "ims_dialog",
        "dlg_refer",
        "documented; ims_dialog exports no dlg_refer",
    ),
    (
        "5.8.8",
        "kex",
        "km_append_branch",
        "documented; kex_mod.c exports no km_append_branch",
    ),
    (
        "5.8.8",
        "math",
        "pow",
        "documented as pow; math.c exports math_pow",
    ),
    (
        "5.8.8",
        "osp",
        "prepareallosproute",
        "heading drops the final s; osp_mod.c exports prepareallosproutes",
    ),
    (
        "5.8.8",
        "pv_headers",
        "pvh_value_exists",
        "documented; pv_headers exports no pvh_value_exists",
    ),
    (
        "5.8.8",
        "siputils",
        "contact_param_decode_uri",
        "documented; siputils exports contact_param_decode_ruri",
    ),
    (
        "5.8.8",
        "sworker",
        "swork_task",
        "documented as swork_task; sworker exports sworker_task",
    ),
    (
        "5.8.8",
        "uid_gflags",
        "set_gflag",
        "documented as set_gflag; uid_gflags exports set_ugflag",
    ),
    (
        "5.8.8",
        "uid_uri_db",
        "check_from",
        "documented; uid_uri_db_mod.c exports only lookup_user and check_uri",
    ),
    (
        "5.8.8",
        "uid_uri_db",
        "check_to",
        "documented; uid_uri_db_mod.c exports only lookup_user and check_uri",
    ),
    (
        "5.8.8",
        "uid_uri_db",
        "does_uri_exist",
        "documented; uid_uri_db_mod.c exports only lookup_user and check_uri",
    ),
    (
        "6.0.8",
        "avpops",
        "avp_printf",
        "documented; avpops exports avp_print, and avp_printf is kex (kex_mod.c)",
    ),
    (
        "6.0.8",
        "cnxcc",
        "cnxcc_set_max_channel",
        "heading drops the final s; cnxcc exports cnxcc_set_max_channels",
    ),
    (
        "6.0.8",
        "db2_ops",
        "db_close",
        "exported as MODULE_NAME2 \"_close\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.0.8",
        "db2_ops",
        "db_first",
        "exported as MODULE_NAME2 \"_first\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.0.8",
        "db2_ops",
        "db_foreach",
        "exported as MODULE_NAME2 \"_foreach\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.0.8",
        "db2_ops",
        "db_next",
        "exported as MODULE_NAME2 \"_next\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.0.8",
        "db2_ops",
        "db_proper",
        "exported as MODULE_NAME2 \"_proper\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.0.8",
        "db2_ops",
        "db_query",
        "exported as MODULE_NAME2 \"_query\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.0.8",
        "db2_ops",
        "db_seek",
        "exported as MODULE_NAME2 \"_seek\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.0.8",
        "ims_auth",
        "ims_www_authorize",
        "documented; ims_auth exports ims_www_authenticate",
    ),
    (
        "6.0.8",
        "ims_dialog",
        "dlg_bridge",
        "documented; ims_dialog exports no dlg_bridge",
    ),
    (
        "6.0.8",
        "ims_dialog",
        "dlg_manage",
        "documented; ims_dialog exports no dlg_manage",
    ),
    (
        "6.0.8",
        "ims_dialog",
        "dlg_refer",
        "documented; ims_dialog exports no dlg_refer",
    ),
    (
        "6.0.8",
        "kex",
        "km_append_branch",
        "documented; kex_mod.c exports no km_append_branch",
    ),
    (
        "6.0.8",
        "math",
        "pow",
        "documented as pow; math.c exports math_pow",
    ),
    (
        "6.0.8",
        "pv_headers",
        "pvh_value_exists",
        "documented; pv_headers exports no pvh_value_exists",
    ),
    (
        "6.0.8",
        "siputils",
        "contact_param_decode_uri",
        "documented; siputils exports contact_param_decode_ruri",
    ),
    (
        "6.0.8",
        "uid_gflags",
        "set_gflag",
        "documented as set_gflag; uid_gflags exports set_ugflag",
    ),
    (
        "6.0.8",
        "uid_uri_db",
        "check_from",
        "documented; uid_uri_db_mod.c exports only lookup_user and check_uri",
    ),
    (
        "6.0.8",
        "uid_uri_db",
        "check_to",
        "documented; uid_uri_db_mod.c exports only lookup_user and check_uri",
    ),
    (
        "6.0.8",
        "uid_uri_db",
        "does_uri_exist",
        "documented; uid_uri_db_mod.c exports only lookup_user and check_uri",
    ),
    (
        "6.1.5",
        "avpops",
        "avp_printf",
        "documented; avpops exports avp_print, and avp_printf is kex (kex_mod.c)",
    ),
    (
        "6.1.5",
        "cnxcc",
        "cnxcc_set_max_channel",
        "heading drops the final s; cnxcc exports cnxcc_set_max_channels",
    ),
    (
        "6.1.5",
        "db2_ops",
        "db_close",
        "exported as MODULE_NAME2 \"_close\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.1.5",
        "db2_ops",
        "db_first",
        "exported as MODULE_NAME2 \"_first\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.1.5",
        "db2_ops",
        "db_foreach",
        "exported as MODULE_NAME2 \"_foreach\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.1.5",
        "db2_ops",
        "db_next",
        "exported as MODULE_NAME2 \"_next\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.1.5",
        "db2_ops",
        "db_proper",
        "exported as MODULE_NAME2 \"_proper\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.1.5",
        "db2_ops",
        "db_query",
        "exported as MODULE_NAME2 \"_query\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.1.5",
        "db2_ops",
        "db_seek",
        "exported as MODULE_NAME2 \"_seek\" (db2_ops.c, MODULE_NAME2 is \"db\"): built by C string concatenation",
    ),
    (
        "6.1.5",
        "dialog",
        "dlg_refer_dcid",
        "documented; dialog exports dlg_refer and dlg_refer_cid, no dlg_refer_dcid",
    ),
    (
        "6.1.5",
        "ims_auth",
        "ims_www_authorize",
        "documented; ims_auth exports ims_www_authenticate",
    ),
    (
        "6.1.5",
        "ims_dialog",
        "dlg_bridge",
        "documented; ims_dialog exports no dlg_bridge",
    ),
    (
        "6.1.5",
        "ims_dialog",
        "dlg_manage",
        "documented; ims_dialog exports no dlg_manage",
    ),
    (
        "6.1.5",
        "ims_dialog",
        "dlg_refer",
        "documented; ims_dialog exports no dlg_refer",
    ),
    (
        "6.1.5",
        "kex",
        "km_append_branch",
        "documented; kex_mod.c exports no km_append_branch",
    ),
    (
        "6.1.5",
        "math",
        "pow",
        "documented as pow; math.c exports math_pow",
    ),
    (
        "6.1.5",
        "siputils",
        "contact_param_decode_uri",
        "documented; siputils exports contact_param_decode_ruri",
    ),
    (
        "6.1.5",
        "uid_gflags",
        "set_gflag",
        "documented as set_gflag; uid_gflags exports set_ugflag",
    ),
    (
        "6.1.5",
        "uid_uri_db",
        "check_from",
        "documented; uid_uri_db_mod.c exports only lookup_user and check_uri",
    ),
    (
        "6.1.5",
        "uid_uri_db",
        "check_to",
        "documented; uid_uri_db_mod.c exports only lookup_user and check_uri",
    ),
    (
        "6.1.5",
        "uid_uri_db",
        "does_uri_exist",
        "documented; uid_uri_db_mod.c exports only lookup_user and check_uri",
    ),
];

fn trees() -> Vec<(String, std::path::PathBuf)> {
    common::required_env("KAMAILIO_LSP_TEST_TREES")
        .split(',')
        .map(|pair| {
            let (v, t) = pair.split_once('=').expect("<version>=<tree>");
            (v.to_string(), std::path::PathBuf::from(t))
        })
        .collect()
}

fn c_text(tree: &std::path::Path, module: &str) -> String {
    let dir = tree.join("src").join("modules").join(module);
    let mut out = String::new();
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "c" || x == "h"))
        .collect();
    files.sort();
    for p in files {
        out.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
    }
    out
}

/// Every function in the catalogue is a name its module's C sources
/// contain, for every pinned release: the vendored catalogue, and what
/// a configured tree yields at runtime.
#[test]
fn no_catalogue_function_is_absent_from_its_modules_c_sources() {
    let mut phantom: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (version, tree) in trees() {
        let vendored = kamailio_lsp::catalog::builtin_modules_at(&version)
            .unwrap_or_else(|| panic!("{version} is a pinned release"));
        let harvested = kamailio_lsp::catalog::harvest_tree(&tree);
        let mut found: Vec<String> = Vec::new();
        for (what, mods) in [("", &vendored.modules), (" (harvest)", &harvested)] {
            for m in mods {
                let c = c_text(&tree, &m.name);
                if c.is_empty() {
                    continue;
                }
                for f in &m.functions {
                    checked += 1;
                    if !c.contains(&format!("\"{}\"", f.name)) {
                        found.push(format!("{version}{what} {}::{}", m.name, f.name));
                    }
                }
            }
        }
        for (v, module, f, why) in UPSTREAM_DOC_ERRORS {
            if *v != version {
                continue;
            }
            for what in ["", " (harvest)"] {
                let entry = format!("{version}{what} {module}::{f}");
                assert!(
                    found.contains(&entry),
                    "the documented exception {entry} ({why}) no longer fires: \
                     remove it from UPSTREAM_DOC_ERRORS"
                );
                found.retain(|p| *p != entry);
            }
        }
        phantom.extend(found);
    }
    // POSITIVE CONTROL: a reading that found no functions would pass
    assert!(checked > 6000, "suspiciously few functions read: {checked}");
    assert!(
        phantom.is_empty(),
        "{} catalogue function(s) the module's C sources never name:\n{}",
        phantom.len(),
        phantom.join("\n")
    );
}

/// The aliases the catalogue carries are exactly the ones the C export
/// tables define, in every pinned release, vendored and harvested.
/// Pinned as a set, so a new alias upstream, or a reading that finds
/// none, fails here. Compared as sorted sets: the order is not the claim.
#[test]
fn the_catalogue_carries_exactly_the_aliases_the_c_sources_define() {
    // each measured in the C export tables: the same handler and the
    // same parameter count as a documented function, under a name the
    // README does not document (e.g. sl.c 6.1.5 lines 103-104, tm.c
    // 413/418, kex_mod.c 98/100, dispatcher.c 261/263)
    let want: &[&str] = &[
        "5.8.8 auth::pv_proxy_authorize",
        "5.8.8 auth::pv_www_authorize",
        "5.8.8 avp::set_attr",
        "5.8.8 dispatcher::ds_list_exist",
        "5.8.8 kex::avp_printf",
        "5.8.8 sl::sl_reply",
        "5.8.8 tm::t_disable_e2e_cancel_reason",
        "5.8.8 uid_auth_db::proxy_authenticate",
        "5.8.8 uid_auth_db::www_authenticate",
        "6.0.8 auth::pv_proxy_authorize",
        "6.0.8 auth::pv_www_authorize",
        "6.0.8 avp::set_attr",
        "6.0.8 dispatcher::ds_list_exist",
        "6.0.8 kex::avp_printf",
        "6.0.8 sl::sl_reply",
        "6.0.8 tm::t_disable_e2e_cancel_reason",
        "6.0.8 uid_auth_db::proxy_authenticate",
        "6.0.8 uid_auth_db::www_authenticate",
        "6.1.5 auth::pv_proxy_authorize",
        "6.1.5 auth::pv_www_authorize",
        "6.1.5 avp::set_attr",
        "6.1.5 dispatcher::ds_list_exist",
        "6.1.5 kex::avp_printf",
        "6.1.5 sl::sl_reply",
        "6.1.5 tm::t_disable_e2e_cancel_reason",
        "6.1.5 uid_auth_db::proxy_authenticate",
        "6.1.5 uid_auth_db::www_authenticate",
    ];
    for (version, tree) in trees() {
        let vendored = kamailio_lsp::catalog::builtin_modules_at(&version)
            .unwrap_or_else(|| panic!("{version} is a pinned release"));
        let harvested = kamailio_lsp::catalog::harvest_tree(&tree);
        let v = version.as_str();
        for (what, mods) in [("vendored", &vendored.modules), ("harvest", &harvested)] {
            let mut aliases: Vec<String> = mods
                .iter()
                .flat_map(|m| {
                    m.functions
                        .iter()
                        .filter(|f| f.doc.starts_with("Alias of `"))
                        .map(move |f| format!("{v} {}::{}", m.name, f.name))
                })
                .collect();
            aliases.sort();
            let expected: Vec<String> = want
                .iter()
                .filter(|w| w.starts_with(&format!("{version} ")))
                .map(|s| s.to_string())
                .collect();
            assert_eq!(aliases, expected, "{version} {what}");
        }
    }
}

/// (release, module, function, why) for each function the docbook
/// documents that the module's README, which the catalogue is harvested
/// from, does not. Each must keep firing.
const README_OMITS: &[(&str, &str, &str, &str)] = &[
    (
        "5.8.8",
        "presence",
        "handle_publish",
        "the 5.8.8 README has no Functions chapter; doc/presence_admin.xml documents it",
    ),
    (
        "5.8.8",
        "presence",
        "handle_subscribe",
        "the 5.8.8 README has no Functions chapter; doc/presence_admin.xml documents it",
    ),
    (
        "5.8.8",
        "presence",
        "pres_auth_status",
        "the 5.8.8 README has no Functions chapter; doc/presence_admin.xml documents it",
    ),
    (
        "5.8.8",
        "presence",
        "pres_has_subscribers",
        "the 5.8.8 README has no Functions chapter; doc/presence_admin.xml documents it",
    ),
    (
        "5.8.8",
        "presence",
        "pres_refresh_watchers",
        "the 5.8.8 README has no Functions chapter; doc/presence_admin.xml documents it",
    ),
    (
        "5.8.8",
        "presence",
        "pres_update_watchers",
        "the 5.8.8 README has no Functions chapter; doc/presence_admin.xml documents it",
    ),
    (
        "5.8.8",
        "tls_wolfssl",
        "is_peer_verified",
        "the README has no Functions chapter; doc/functions.xml documents it",
    ),
    (
        "5.8.8",
        "tls_wolfssl",
        "tls_set_connect_server_id",
        "the README has no Functions chapter; doc/functions.xml documents it",
    ),
    (
        "6.0.8",
        "tls_wolfssl",
        "is_peer_verified",
        "the README has no Functions chapter; doc/functions.xml documents it",
    ),
    (
        "6.0.8",
        "tls_wolfssl",
        "tls_set_connect_server_id",
        "the README has no Functions chapter; doc/functions.xml documents it",
    ),
    (
        "6.1.5",
        "tls_wolfssl",
        "is_peer_verified",
        "the README has no Functions chapter; doc/functions.xml documents it",
    ),
    (
        "6.1.5",
        "tls_wolfssl",
        "tls_set_connect_server_id",
        "the README has no Functions chapter; doc/functions.xml documents it",
    ),
];

/// The other direction: every function a module's docbook documents as
/// one, and its C sources name, is in the catalogue, for every pinned
/// release.
///
/// The oracle reads the docbook sources independently of the README
/// harvester: a section whose id is `<module>.f.<name>` (the convention
/// in 152 of the 256 modules of 6.1.5) and whose title opens with a
/// `<function>` element. `tm`'s C API (`doc/api.xml`) carries no `.f.`
/// ids, so it is not asked for. Only names the C sources contain as a
/// string literal count: a name the documentation invents is not held
/// against the catalogue.
#[test]
fn the_catalogue_contains_every_function_the_docbook_documents() {
    let section = regex::Regex::new(
        r#"(?s)<section\s+id="[A-Za-z0-9_]+\.f\.[^"]*"[^>]*>\s*<title>\s*<function[^>]*>\s*([A-Za-z_][A-Za-z0-9_]*)"#,
    )
    .unwrap();
    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (version, tree) in trees() {
        let vendored = kamailio_lsp::catalog::builtin_modules_at(&version)
            .unwrap_or_else(|| panic!("{version} is a pinned release"));
        let harvested = kamailio_lsp::catalog::harvest_tree(&tree);
        let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(tree.join("src").join("modules"))
            .expect("a Kamailio source tree has src/modules/")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("README").is_file())
            .collect();
        dirs.sort();
        for dir in dirs {
            let module = dir.file_name().unwrap().to_string_lossy().into_owned();
            let c = c_text(&tree, &module);
            let mut xml = String::new();
            for e in std::fs::read_dir(dir.join("doc"))
                .into_iter()
                .flatten()
                .flatten()
            {
                let name = e.file_name().to_string_lossy().into_owned();
                // a Developer Guide documents the C API other modules
                // call (`peerstate_devel.xml`'s `bind_peerstate`)
                if e.path().extension().is_some_and(|x| x == "xml") && !name.contains("devel") {
                    xml.push_str(&std::fs::read_to_string(e.path()).unwrap_or_default());
                }
            }
            let mut names: Vec<String> = section
                .captures_iter(&xml)
                .map(|c| c[1].to_string())
                .filter(|n| c.contains(&format!("\"{n}\"")))
                .collect();
            names.sort();
            names.dedup();
            for f in names {
                checked += 1;
                for (what, mods) in [("vendored", &vendored.modules), ("harvest", &harvested)] {
                    let found = mods
                        .iter()
                        .find(|m| m.name == module)
                        .is_some_and(|m| m.functions.iter().any(|x| x.name == f));
                    if !found {
                        missing.push(format!("{version} {what}: {module}::{f}"));
                    }
                }
            }
        }
    }
    for (v, module, f, why) in README_OMITS {
        for what in ["vendored", "harvest"] {
            let entry = format!("{v} {what}: {module}::{f}");
            assert!(
                missing.contains(&entry),
                "the documented exception {entry} ({why}) no longer fires: \
                 remove it from README_OMITS"
            );
            missing.retain(|p| *p != entry);
        }
    }
    // POSITIVE CONTROL: an oracle that stopped reading would pass
    assert!(checked > 1500, "suspiciously few functions read: {checked}");
    assert!(
        missing.is_empty(),
        "{} documented, exported function(s) absent from the catalogue:\n{}",
        missing.len(),
        missing.join("\n")
    );
}
