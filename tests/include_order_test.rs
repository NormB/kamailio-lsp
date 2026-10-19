//! Where a relative `include_file` opens.
//!
//! `sr_push_yy_state` (`src/core/cfg.lex`) first opens the path as
//! written, which resolves against the process's working directory,
//! and only when that fails tries it beside the file being read.
//! `kamailio -c` 6.1.5, run in the main config's directory as this
//! server and `check` run it (2026-10-19):
//!
//! - `main.cfg` includes `inc/a.cfg`, which includes `inc/b.cfg`:
//!   accepted (`config file ok`), so `inc/b.cfg` is found against the
//!   main config's directory.  Run from `/`, the same config fails
//!   with `cannot open included file: inc/b.cfg (<dir>/inc/inc/b.cfg)`.
//! - With both `x.cfg` and `inc/x.cfg` present and `inc/a.cfg`
//!   including `x.cfg`, the main directory's `x.cfg` is the one read:
//!   its `route[B]` collides with `main.cfg`'s (`duplicate route`).

mod common;

use kamailio_lsp::logic::{IncludeGraph, analyzer_diagnostics_in_closure, include_closure};
use std::path::{Path, PathBuf};

fn loader(files: &[(PathBuf, String)]) -> impl Fn(&Path) -> Option<String> + '_ {
    move |p: &Path| files.iter().find(|(q, _)| q == p).map(|(_, t)| t.clone())
}

#[test]
fn a_nested_include_resolves_against_the_main_configs_directory_first() {
    let main = PathBuf::from("/k/main.cfg");
    let files = vec![
        (
            main.clone(),
            "include_file \"inc/a.cfg\"\nrequest_route {\n    route(B);\n}\n".to_string(),
        ),
        (
            PathBuf::from("/k/inc/a.cfg"),
            "include_file \"inc/b.cfg\"\n".to_string(),
        ),
        (
            PathBuf::from("/k/inc/b.cfg"),
            "route[B] {\n    exit;\n}\n".to_string(),
        ),
    ];
    let closure = include_closure(&main, &files[0].1, &loader(&files));
    let names: Vec<&Path> = closure.iter().map(|(p, _)| p.as_path()).collect();
    assert_eq!(
        names,
        vec![
            Path::new("/k/main.cfg"),
            Path::new("/k/inc/a.cfg"),
            Path::new("/k/inc/b.cfg"),
        ]
    );
    let warnings: Vec<String> = analyzer_diagnostics_in_closure(&closure, &main, &files[0].1)
        .into_iter()
        .map(|d| d.message)
        .collect();
    assert_eq!(warnings, Vec::<String>::new());
}

#[test]
fn the_main_directory_wins_when_both_files_exist() {
    let main = PathBuf::from("/k/main.cfg");
    let files = vec![
        (main.clone(), "include_file \"inc/a.cfg\"\n".to_string()),
        (
            PathBuf::from("/k/inc/a.cfg"),
            "include_file \"x.cfg\"\n".to_string(),
        ),
        (
            PathBuf::from("/k/x.cfg"),
            "route[TOP] {\n    exit;\n}\n".to_string(),
        ),
        (
            PathBuf::from("/k/inc/x.cfg"),
            "route[BESIDE] {\n    exit;\n}\n".to_string(),
        ),
    ];
    let closure = include_closure(&main, &files[0].1, &loader(&files));
    let names: Vec<&Path> = closure.iter().map(|(p, _)| p.as_path()).collect();
    assert!(names.contains(&Path::new("/k/x.cfg")), "{names:?}");
    assert!(!names.contains(&Path::new("/k/inc/x.cfg")), "{names:?}");
}

#[test]
fn beside_the_including_file_is_the_fallback() {
    let main = PathBuf::from("/k/main.cfg");
    let files = vec![
        (main.clone(), "include_file \"inc/a.cfg\"\n".to_string()),
        (
            PathBuf::from("/k/inc/a.cfg"),
            "include_file \"b.cfg\"\n".to_string(),
        ),
        (
            PathBuf::from("/k/inc/b.cfg"),
            "route[B] {\n    exit;\n}\n".to_string(),
        ),
    ];
    let closure = include_closure(&main, &files[0].1, &loader(&files));
    assert_eq!(closure.len(), 3);
}

#[test]
fn the_graph_finds_the_root_of_a_root_relative_nested_include() {
    let scratch = common::Scratch::new("kamlsp-incorder");
    let dir = scratch.path().to_path_buf();
    std::fs::create_dir_all(dir.join("inc")).unwrap();
    let w = |p: &str, t: &str| {
        std::fs::write(dir.join(p), t).unwrap();
        (dir.join(p), t.to_string())
    };
    let configs = vec![
        w("main.cfg", "include_file \"inc/a.cfg\"\n"),
        w("inc/a.cfg", "include_file \"inc/b.cfg\"\n"),
        w("inc/b.cfg", "route[B] {\n    exit;\n}\n"),
    ];
    let g = IncludeGraph::build(&configs);
    assert_eq!(
        g.analysis_root(&dir.join("inc/b.cfg")),
        Some(dir.join("main.cfg"))
    );
}
