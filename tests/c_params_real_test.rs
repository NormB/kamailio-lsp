//! The C-table extraction against the real Kamailio tree.
//!
//! `c_params_test.rs` proves the parser against fixtures. These prove
//! it against the thing it actually reads, and they carry the positive
//! controls: a derived rule that silently stopped matching would
//! otherwise excuse everything and report green.

mod common;

use kamailio_lsp::catalog::{builtin_modules, param_names_from_c};
use std::path::Path;

fn tree() -> String {
    common::required_env("KAMAILIO_LSP_TEST_TREE")
}

fn module_dirs(root: &Path) -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<_> = std::fs::read_dir(root.join("src").join("modules"))
        .expect("a Kamailio source tree has src/modules/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// POSITIVE CONTROL. Everything else here is phrased as "absent from
/// the C tables means X". Were the extraction to quietly stop
/// matching, every such rule would pass while proving nothing.
///
/// Measured against 6.1.4: 255 module directories, 227 of which export
/// at least one parameter. The 28 that export none are the
/// function-only modules — `textops`, `ipops`, `math`, `jansson` and
/// the like.
#[test]
fn the_extraction_still_reads_the_tree() {
    let root = tree();
    let root = Path::new(&root);
    let dirs = module_dirs(root);
    assert!(
        dirs.len() > 200,
        "suspiciously few module directories: {}",
        dirs.len()
    );

    let mut with_tables = 0usize;
    let mut total = 0usize;
    for dir in &dirs {
        let found = param_names_from_c(dir, root);
        if !found.names.is_empty() {
            with_tables += 1;
            total += found.names.len();
        }
    }
    eprintln!(
        "{with_tables}/{} modules export a parameter, {total} names",
        dirs.len()
    );
    assert!(
        with_tables >= 220,
        "only {with_tables} modules parsed a table; the extraction has regressed"
    );
    assert!(
        total >= 2300,
        "only {total} parameter names extracted; the extraction has regressed"
    );
}

/// POSITIVE CONTROL. A floor on counts survives a parser returning
/// plausible rubbish, so name specific parameters too.
#[test]
fn known_parameters_are_extracted() {
    let root = tree();
    let root = Path::new(&root);
    for (module, param) in [
        ("kazoo", "amqp_consumer_ack_timeout_sec"),
        ("kazoo", "amqp_consumer_ack_timeout_micro"),
        ("msilo", "sc_status"),
        ("ndb_cassandra", "port"),
        ("dispatcher", "ds_ping_interval"),
    ] {
        let found = param_names_from_c(&root.join("src/modules").join(module), root);
        assert!(
            found.names.iter().any(|n| n == param),
            "{module}::{param} must come out of the C tables, got {} names",
            found.names.len()
        );
    }
}

/// `matrix` assembles its ENTIRE table from macros in `db_matrix.h` —
/// `matrix_DB_URL`, `matrix_DB_TABLE`, `matrix_DB_COLS`. Read without
/// expanding them the module exports nothing at all, and every one of
/// its parameters would be reported as absent.
#[test]
fn macro_assembled_tables_are_expanded() {
    let root = tree();
    let root = Path::new(&root);
    let found = param_names_from_c(&root.join("src/modules/matrix"), root);
    assert!(found.complete, "matrix: table did not fully resolve");
    for param in ["matrix_table", "matrix_first_col", "matrix_res_col"] {
        assert!(
            found.names.iter().any(|n| n == param),
            "matrix::{param} comes from a macro and must survive expansion, got {:?}",
            found.names
        );
    }
}

/// The cfg framework is a different namespace. `dispatcher` registers
/// `probing_threshold` for RPC (`cfg.set`) and exports
/// `ds_probing_threshold` for `modparam`. Harvesting `cfg_def_t` would
/// put names in the catalogue that `modparam()` rejects.
#[test]
fn the_cfg_framework_namespace_is_not_harvested() {
    let root = tree();
    let root = Path::new(&root);
    let found = param_names_from_c(&root.join("src/modules/dispatcher"), root);
    assert!(
        found.names.iter().any(|n| n == "ds_probing_threshold"),
        "the modparam spelling must be present"
    );
    assert!(
        !found.names.iter().any(|n| n == "probing_threshold"),
        "the RPC-only cfg spelling must NOT be present"
    );
}

/// No catalogue entry names a parameter the module never exported.
#[test]
fn the_catalogue_contains_no_parameter_absent_from_the_c_tables() {
    let root = tree();
    let root = Path::new(&root);
    let catalogue = builtin_modules();

    let mut phantoms: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for dir in module_dirs(root) {
        let module = dir.file_name().unwrap().to_string_lossy().into_owned();
        let found = param_names_from_c(&dir, root);
        if found.names.is_empty() || !found.complete {
            continue;
        }
        let Some(doc) = catalogue.modules.iter().find(|m| m.name == module) else {
            continue;
        };
        for p in &doc.params {
            checked += 1;
            if !found.names.contains(&p.name) {
                phantoms.push(format!("{module}::{}", p.name));
            }
        }
    }
    assert!(
        checked > 1500,
        "suspiciously few entries checked: {checked}"
    );
    assert!(
        phantoms.is_empty(),
        "{} catalogue entr(ies) name a parameter absent from the module's C tables:\n{}",
        phantoms.len(),
        phantoms.join("\n")
    );
    eprintln!("{checked} catalogue entries all trace to a C parameter table");
}

/// The headline defect. `kazoo`'s heading documents
/// `amqp_consumer_ack_timeout`, which the module never exported, while
/// the `_sec` and `_micro` parameters it really has went undocumented.
/// The server offered the phantom in completion and warned at both
/// real ones.
#[test]
fn the_kazoo_phantom_is_gone_and_the_real_pair_is_present() {
    let catalogue = builtin_modules();
    let kazoo = catalogue
        .modules
        .iter()
        .find(|m| m.name == "kazoo")
        .expect("kazoo must be in the catalogue");
    let has = |n: &str| kazoo.params.iter().any(|p| p.name == n);
    for phantom in [
        "amqp_consumer_ack_timeout",
        "amqp_interprocess_timeout",
        "amqp_query_timeout",
        "amqp_waitframe_timeout",
    ] {
        assert!(!has(phantom), "kazoo::{phantom} is not exported by kazoo");
    }
    for real in [
        "amqp_consumer_ack_timeout_sec",
        "amqp_consumer_ack_timeout_micro",
    ] {
        assert!(has(real), "kazoo::{real} is exported and must be present");
    }
}

/// A parameter the C table exports and no heading documents is in the
/// catalogue, carrying a doc line saying where it came from.
#[test]
fn undocumented_parameters_are_harvested_from_c() {
    let catalogue = builtin_modules();
    for (module, param) in [("msilo", "sc_status"), ("ndb_cassandra", "port")] {
        let doc = catalogue
            .modules
            .iter()
            .find(|m| m.name == module)
            .unwrap_or_else(|| panic!("{module} must be in the catalogue"));
        let item = doc
            .params
            .iter()
            .find(|p| p.name == param)
            .unwrap_or_else(|| panic!("{module}::{param} must be harvested from the C table"));
        assert!(
            item.doc.contains("not documented in the module README"),
            "{module}::{param} should say it is undocumented, got {:?}",
            item.doc
        );
    }
}

/// A module exporting no parameter keeps its README harvest exactly.
#[test]
fn modules_without_a_table_are_left_alone() {
    let root = tree();
    let root = Path::new(&root);
    let mut seen = 0usize;
    for module in ["textops", "ipops", "math", "jansson"] {
        let dir = root.join("src/modules").join(module);
        if !dir.is_dir() {
            continue;
        }
        seen += 1;
        let found = param_names_from_c(&dir, root);
        assert!(
            found.names.is_empty(),
            "{module} was expected to export no parameters, got {:?}",
            found.names
        );
    }
    assert!(seen >= 3, "the fixture modules are missing from the tree");
}

/// The live path. `builtin_modules()` reads a JSON generated ahead of
/// time, so a reconciliation that stopped running would leave that
/// file — and every test reading it — looking correct. This exercises
/// `harvest_tree`, which is what a user pointing `kamailioSrc` at
/// their own tree gets.
#[test]
fn harvesting_the_tree_directly_reconciles_against_c() {
    let root = tree();
    let root = Path::new(&root);
    let harvested = kamailio_lsp::catalog::harvest_tree(root);
    assert!(
        harvested.len() > 200,
        "suspiciously few modules harvested: {}",
        harvested.len()
    );
    let params = |m: &str| {
        harvested
            .iter()
            .find(|d| d.name == m)
            .unwrap_or_else(|| panic!("{m} must be harvested"))
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
    };
    assert!(
        params("matrix").iter().any(|n| n == "matrix_table"),
        "matrix::matrix_table comes from a macro and must survive harvesting"
    );
    assert!(
        params("kazoo")
            .iter()
            .any(|n| n == "amqp_consumer_ack_timeout_sec"),
        "the real kazoo parameter must be harvested"
    );
    assert!(
        !params("kazoo")
            .iter()
            .any(|n| n == "amqp_consumer_ack_timeout"),
        "the kazoo phantom must not be harvested"
    );

    let mut phantoms: Vec<String> = Vec::new();
    for doc in &harvested {
        let found = param_names_from_c(&root.join("src/modules").join(&doc.name), root);
        if found.names.is_empty() || !found.complete {
            continue;
        }
        for p in &doc.params {
            if !found.names.contains(&p.name) {
                phantoms.push(format!("{}::{}", doc.name, p.name));
            }
        }
    }
    assert!(
        phantoms.is_empty(),
        "{} harvested entr(ies) name a parameter absent from the C tables:\n{}",
        phantoms.len(),
        phantoms.join("\n")
    );
}

/// The completeness guard, on a fixture that can actually violate it.
///
/// No module in the 6.1.4 tree parses incompletely, so nothing there
/// exercises this branch: a test reading only the real tree passes
/// whether the guard is right or wrong. `matrix` is why the guard
/// exists — a table that is entirely macros yields a short name list
/// when they cannot be resolved, and dropping against a short list
/// deletes real parameters. These two synthetic modules differ only in
/// whether the splice resolves.
#[test]
fn an_unresolved_splice_drops_nothing() {
    let dir = std::env::temp_dir().join(format!("kamlsp-cguard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let readme = "m\n\n1.3. Parameters\n\n1.3.1. documented_only (int)\n\nIn the README, never exported.\n\n1.3.2. really_exported (int)\n\nIn both.\n";
    for (module, table) in [
        (
            "unresolved",
            "\t{\"really_exported\", PARAM_INT, &x},\n\tSOME_SHARED_MACRO\n",
        ),
        ("resolved", "\t{\"really_exported\", PARAM_INT, &x},\n"),
    ] {
        let md = dir.join("src").join("modules").join(module);
        std::fs::create_dir_all(&md).expect("fixture dirs");
        std::fs::write(md.join("README"), readme).expect("fixture README");
        std::fs::write(
            md.join("mod.c"),
            format!("static param_export_t params[] = {{\n{table}\t{{0, 0, 0}}\n}};\n"),
        )
        .expect("fixture source");
    }

    let harvested = kamailio_lsp::catalog::harvest_tree(&dir);
    let params = |m: &str| {
        harvested
            .iter()
            .find(|d| d.name == m)
            .unwrap_or_else(|| panic!("{m} must be harvested"))
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
    };

    let unresolved = params("unresolved");
    assert!(
        unresolved.iter().any(|n| n == "documented_only"),
        "an unresolved splice means the name list may be short, so nothing may be dropped; got {unresolved:?}"
    );
    let resolved = params("resolved");
    assert!(
        !resolved.iter().any(|n| n == "documented_only"),
        "a fully resolved table is authoritative and the phantom must go; got {resolved:?}"
    );
    assert!(
        resolved.iter().any(|n| n == "really_exported"),
        "the exported parameter must survive; got {resolved:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A conditional entry must not make a table look unresolved.
///
/// `tm` guards an entry behind `USE_DNS_FAILOVER` and `tls` behind
/// `KSR_SSL_ENGINE`. Read without skipping the directive, `ifdef`
/// parses as a bare identifier — a macro splice this parser cannot
/// find — and the table is marked incomplete. Every name is still
/// collected, so a test that only checks names passes; what is lost is
/// the authority to drop a phantom from those modules.
#[test]
fn conditional_tables_still_resolve_completely() {
    let root = tree();
    let root = Path::new(&root);
    for module in ["tm", "tls"] {
        let dir = root.join("src/modules").join(module);
        if !dir.is_dir() {
            continue;
        }
        let found = param_names_from_c(&dir, root);
        assert!(
            !found.names.is_empty(),
            "{module} must export parameters, got none"
        );
        assert!(
            found.complete,
            "{module}: a `#ifdef` inside the table must not read as an unresolved splice"
        );
    }
}
