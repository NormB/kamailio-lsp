//! `scripts/proof-env.sh` must prove against the wiki CI proves against.
//!
//! The wiki is not a tag: CI clones its current HEAD on every run. The
//! script used to reuse any existing local checkout as it was, so a
//! local run could prove against a wiki two months older than the one
//! CI used (2026-10-19: a checkout from 2026-08-10 against an upstream
//! last changed 2026-10-16) and pass or fail for reasons CI never saw.
//!
//! Everything here is local: the "upstream" wiki is a repository made
//! by the test, and the Kamailio install and older trees are the
//! minimal stand-ins the script checks for, so it builds and clones
//! nothing else.

mod common;

use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=proof-env test",
            "-c",
            "user.email=proof-env@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit_cookbook(upstream: &Path, text: &str) -> String {
    let page = upstream.join("docs/cookbooks/6.1.x/core.md");
    std::fs::create_dir_all(page.parent().unwrap()).unwrap();
    std::fs::write(&page, text).unwrap();
    git(upstream, &["add", "-A"]);
    git(upstream, &["commit", "-q", "-m", text]);
    git(upstream, &["rev-parse", "HEAD"])
}

fn run_script(root: &Path, upstream: &Path) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/proof-env.sh");
    let out = Command::new("bash")
        .arg(&script)
        .env("PROOF_ROOT", root)
        .env("KAMAILIO_TAG", "9.9.9")
        .env("KAMAILIO_OLDER_TAGS", "9.9.8")
        .env(
            "KAMAILIO_WIKI_URL",
            format!("file://{}", upstream.display()),
        )
        .env_remove("GITHUB_ENV")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .expect("the script runs");
    assert!(
        out.status.success(),
        "proof-env.sh failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn an_existing_wiki_checkout_follows_upstream() {
    let scratch = common::Scratch::new("kamlsp-proof-env");
    let upstream = scratch.join("upstream-wiki");
    let root = scratch.join("proof");
    std::fs::create_dir_all(&upstream).unwrap();
    git(&upstream, &["init", "-q"]);

    // what the script checks for before it would build or clone Kamailio
    let bin = root.join("inst-9.9.9/sbin/kamailio");
    std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
    std::fs::write(&bin, "#!/bin/sh\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let tm = root.join("inst-9.9.9/lib64/kamailio/modules/tm.so");
    std::fs::create_dir_all(tm.parent().unwrap()).unwrap();
    std::fs::write(&tm, "").unwrap();
    std::fs::create_dir_all(root.join("kamailio-9.9.8/src/modules")).unwrap();

    let first = commit_cookbook(&upstream, "first");
    run_script(&root, &upstream);
    let wiki = root.join("kamailio-wiki");
    // POSITIVE CONTROL: the script cloned from the test's upstream
    assert_eq!(
        git(&wiki, &["rev-parse", "HEAD"]),
        first,
        "first run must clone upstream"
    );

    let second = commit_cookbook(&upstream, "second");
    run_script(&root, &upstream);
    assert_eq!(
        git(&wiki, &["rev-parse", "HEAD"]),
        second,
        "upstream moved on; the existing checkout must follow it, as a fresh CI clone would"
    );
    assert_eq!(
        std::fs::read_to_string(wiki.join("docs/cookbooks/6.1.x/core.md")).unwrap(),
        "second"
    );
}
