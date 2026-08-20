//! Per-(URI, version) memoization of the per-document computations
//! (route blocks, route refs, semantic spans).

use kamailio_lsp::logic::{DocCache, doc_index_builds};
use std::sync::Arc;

/// The build counter is process-global and the test harness runs
/// tests in parallel threads: counting tests take this lock so their
/// deltas cannot interleave.
static COUNT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn same_version_reuses_the_computation() {
    let _g = COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cache: DocCache<String> = DocCache::new();
    let text = "route[A] {\n    route(A);\n}\n";
    let before = doc_index_builds();
    let a = cache.get_or_index("u".into(), 1, text);
    let b = cache.get_or_index("u".into(), 1, text);
    assert!(Arc::ptr_eq(&a, &b), "same version must share one index");
    assert_eq!(doc_index_builds() - before, 1, "exactly one build");
    assert_eq!(a.blocks.len(), 1);
    assert_eq!(a.refs.len(), 1);
    assert!(!a.spans.is_empty());
}

#[test]
fn newer_version_evicts_the_stale_index() {
    let _g = COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cache: DocCache<String> = DocCache::new();
    let before = doc_index_builds();
    let v1 = cache.get_or_index("u".into(), 1, "route[A] { exit; }\n");
    let v2 = cache.get_or_index("u".into(), 2, "route[B] { exit; }\n");
    assert!(!Arc::ptr_eq(&v1, &v2));
    assert_eq!(v2.blocks[0].name, "B", "the new version's content wins");
    assert_eq!(doc_index_builds() - before, 2);
    // asking for the new version again stays cached
    let v2b = cache.get_or_index("u".into(), 2, "route[B] { exit; }\n");
    assert!(Arc::ptr_eq(&v2, &v2b));
    assert_eq!(doc_index_builds() - before, 2);
}

#[test]
fn concurrent_same_version_requests_race_safely_to_one_build() {
    let _g = COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cache: Arc<DocCache<String>> = Arc::new(DocCache::new());
    let text = "route[R] {\n    route(R);\n}\n";
    let before = doc_index_builds();
    let mut handles = Vec::new();
    for _ in 0..16 {
        let c = cache.clone();
        handles.push(std::thread::spawn(move || {
            c.get_or_index("k".into(), 7, text)
        }));
    }
    let results: Vec<Arc<_>> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    for r in &results[1..] {
        assert!(Arc::ptr_eq(&results[0], r), "all callers share one index");
    }
    assert_eq!(
        doc_index_builds() - before,
        1,
        "one build despite 16 racers"
    );
}

#[test]
fn eviction_and_adversarial_texts() {
    let _g = COUNT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cache: DocCache<String> = DocCache::new();
    // hostile inputs must not panic and must index as empty-ish
    for text in ["", "\u{0}", "route[\u{0}] {", "\\\\\\", "route['x] {\n"] {
        let idx = cache.get_or_index(format!("k-{}", text.len()), 1, text);
        let _ = (&idx.blocks, &idx.refs, &idx.spans);
    }
    // evict drops the entry: the next get rebuilds
    let before = doc_index_builds();
    let a = cache.get_or_index("e".into(), 1, "route[A] { exit; }\n");
    cache.evict(&"e".into());
    let b = cache.get_or_index("e".into(), 1, "route[A] { exit; }\n");
    assert!(!Arc::ptr_eq(&a, &b), "evicted entries rebuild");
    assert_eq!(doc_index_builds() - before, 2);
}
