//! Catalog cache: harvest results are cached per (source tree, wiki)
//! pair and invalidated when either changes.

use kamailio_lsp::catalog::{CoreDocs, Item, ModuleDoc, load_cached, save_cache, tree_fingerprint};

fn mk_tree(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("kamlsp-cache-tree-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src").join("modules").join("m")).unwrap();
    root
}

fn mk_wiki(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("kamlsp-cache-wiki-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("core.md"), "# Core Cookbook\n").unwrap();
    root
}

fn sample() -> (Vec<ModuleDoc>, CoreDocs) {
    (
        vec![ModuleDoc {
            name: "tm".into(),
            params: vec![Item {
                name: "fr_timer".into(),
                detail: "integer".into(),
                doc: "d".into(),
            }],
            functions: vec![],
        }],
        CoreDocs {
            functions: vec![Item {
                name: "exit".into(),
                detail: "exit".into(),
                doc: "d".into(),
            }],
            params: vec![],
            pvars: vec![],
        },
    )
}

#[test]
fn cache_roundtrips_and_invalidates_on_tree_change() {
    let tree = mk_tree("rt");
    let wiki = mk_wiki("rt");
    let cache = std::env::temp_dir().join(format!("kamlsp-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let (mods, core) = sample();

    // nothing cached yet
    assert!(load_cached(&tree, Some(&wiki), &cache).is_none());

    save_cache(&tree, Some(&wiki), &cache, &mods, &core).expect("save");
    let (m2, c2) = load_cached(&tree, Some(&wiki), &cache).expect("hit");
    assert_eq!(m2, mods);
    assert_eq!(c2.functions[0].name, "exit");

    // change the tree: new module directory => new fingerprint => miss
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::create_dir_all(tree.join("src").join("modules").join("new_mod")).unwrap();
    assert!(
        load_cached(&tree, Some(&wiki), &cache).is_none(),
        "stale cache must miss after the tree changes"
    );

    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&wiki);
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn wiki_identity_is_part_of_the_fingerprint() {
    let tree = mk_tree("wk");
    let wiki_a = mk_wiki("wka");
    let wiki_b = mk_wiki("wkb");
    assert_ne!(
        tree_fingerprint(&tree, Some(&wiki_a)),
        tree_fingerprint(&tree, Some(&wiki_b)),
        "different wikis must not share a cache entry"
    );
    assert_ne!(
        tree_fingerprint(&tree, Some(&wiki_a)),
        tree_fingerprint(&tree, None),
        "wiki-less harvest must not share a cache entry with a wiki'd one"
    );
    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&wiki_a);
    let _ = std::fs::remove_dir_all(&wiki_b);
}

#[test]
fn corrupt_cache_is_a_miss_not_a_panic() {
    let tree = mk_tree("corrupt");
    let cache = std::env::temp_dir().join(format!("kamlsp-cache-c-{}", std::process::id()));
    std::fs::create_dir_all(&cache).unwrap();
    let fp = tree_fingerprint(&tree, None);
    std::fs::write(cache.join(format!("{fp}.json")), b"{not json").unwrap();
    assert!(load_cached(&tree, None, &cache).is_none());
    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn distinct_trees_have_distinct_fingerprints() {
    let a = mk_tree("a");
    let b = mk_tree("b");
    assert_ne!(tree_fingerprint(&a, None), tree_fingerprint(&b, None));
    let _ = std::fs::remove_dir_all(&a);
    let _ = std::fs::remove_dir_all(&b);
}

#[test]
fn cache_writes_are_atomic_and_leave_no_temp_files() {
    let tree = mk_tree("atomic");
    let cache = std::env::temp_dir().join(format!("kamlsp-cache-a-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let (mods, core) = sample();

    // hammer: concurrent writers and readers; a torn write would make
    // a reader parse a partial file (load must be Some(valid) or None,
    // and must never panic)
    let tree2 = tree.clone();
    let cache2 = cache.clone();
    let writer = std::thread::spawn(move || {
        for _ in 0..200 {
            save_cache(&tree2, None, &cache2, &mods, &core).unwrap();
        }
    });
    for _ in 0..200 {
        if let Some((m, _)) = load_cached(&tree, None, &cache) {
            assert_eq!(m[0].name, "tm");
        }
    }
    writer.join().unwrap();

    // after the dust settles: exactly the final artifact, no temp litter
    let leftovers: Vec<String> = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.ends_with(".json"))
        .collect();
    assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");

    // a stray temp file from a crashed writer must be ignored by load
    std::fs::write(cache.join("deadbeef.json.tmp"), b"{torn").unwrap();
    assert!(load_cached(&tree, None, &cache).is_some());

    let _ = std::fs::remove_dir_all(&tree);
    let _ = std::fs::remove_dir_all(&cache);
}
