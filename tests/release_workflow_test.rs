//! Drift gates for the GitHub workflows: fence the release traps that
//! past reruns proved real.  Distribution targets are GitHub releases
//! and Open VSX only — the Microsoft VS Code Marketplace is not a
//! target and no publish logic for it may be added here.

fn workflow(name: &str) -> Option<String> {
    std::fs::read_to_string(format!(
        "{}/.github/workflows/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    ))
    .ok()
}

#[test]
fn openvsx_publish_survives_reruns_and_set_e() {
    let y = workflow("release.yml").expect("release.yml must exist");
    // under `bash -e`, `out=$(cmd); rc=$?` dies before rc is read —
    // the capture must pre-seed rc and use `|| rc=$?`
    let (_, section) = y
        .split_once("OVSX_PAT")
        .expect("Open VSX publish job missing");
    assert!(
        section.contains("rc=0") && section.contains("|| rc=$?"),
        "Open VSX publish loop must capture rc set-e-safely"
    );
    // A missing secret must FAIL, not skip.  It used to skip: the
    // step printed a line and exited 0, so a release could cut the
    // tag, upload every GitHub asset and publish nothing at all, with
    // a green tick over the top.  Every installer would keep resolving
    // to the previous version and nothing anywhere would say why.
    assert!(
        section.contains(r#"if [ -z "$OVSX_PAT" ]"#),
        "the missing-secret branch went missing"
    );
    let missing = section
        .split_once(r#"if [ -z "$OVSX_PAT" ]"#)
        .map(|(_, rest)| rest.split("fi").next().unwrap_or(""))
        .unwrap_or("");
    assert!(
        missing.contains("exit 1") && !missing.contains("exit 0"),
        "a missing OVSX_PAT must fail the job, not skip it silently:\n{missing}"
    );

    // `ovsx` exits 0 having uploaded SOMETHING; the step has to check
    // it uploaded the version that was built, or a packaging slip
    // shipping the previous one passes as a release
    assert!(
        section.contains("$VERSION") && section.contains("without naming"),
        "the publish must assert the version it just uploaded"
    );

    // Uploaded is not installable.  The registry indexes
    // asynchronously — v0.20.0 was accepted immediately and served
    // roughly twenty minutes later — and until it is served, every
    // editor still offers the previous version.  A release is not
    // finished until the thing users install is the thing that was
    // released.
    assert!(
        section.contains("open-vsx.org/api/") && section.contains("still not served"),
        "the step must wait for the registry to actually serve the version"
    );
    // reruns of an already-accepted publish must not fail the job
    assert!(
        y.contains("already published"),
        "Open VSX rerun idempotency missing"
    );
}

#[test]
fn release_reruns_refresh_assets_instead_of_failing() {
    let y = workflow("release.yml").expect("release.yml must exist");
    assert!(
        y.contains("gh release view") && y.contains("--clobber"),
        "a rerun must upload --clobber onto the existing release"
    );
}

#[test]
fn workflows_use_action_versions_that_exist() {
    // actions/upload-artifact@v8 does not exist (v7 is current);
    // referencing it fails every job at "Set up job"
    for f in ["release.yml", "ci.yml", "wiki-sync.yml"] {
        let Some(y) = workflow(f) else { continue };
        assert!(
            !y.contains("upload-artifact@v8"),
            "{f}: actions/upload-artifact@v8 does not exist — use v7"
        );
    }
}

#[test]
fn no_ms_marketplace_publish_logic_exists() {
    // decision 2026-08-20: GitHub releases + Open VSX are the only
    // distribution targets — MS Marketplace logic must not come back
    for f in ["release.yml", "ci.yml", "wiki-sync.yml"] {
        let Some(y) = workflow(f) else { continue };
        for needle in ["VSCE_PAT", "vsce publish"] {
            assert!(
                !y.contains(needle),
                "{f}: '{needle}' found — MS Marketplace publishing is not a target"
            );
        }
    }
}

/// Every workflow file, by name and text. Enumerated rather than
/// listed, so a workflow added later is held to the same rules.
fn all_workflows() -> Vec<(String, String)> {
    let dir = format!("{}/.github/workflows", env!("CARGO_MANIFEST_DIR"));
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{dir}: {e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yml" || x == "yaml"))
        .map(|p| {
            let name = p
                .file_name()
                .expect("a directory entry has a file name")
                .to_string_lossy()
                .into_owned();
            (
                name,
                std::fs::read_to_string(&p).expect("the file is readable UTF-8"),
            )
        })
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no workflows under {dir}");
    out
}

/// A `uses:` line's action reference, if the line has one.
fn uses_ref(line: &str) -> Option<&str> {
    let t = line.trim_start().trim_start_matches("- ");
    t.strip_prefix("uses:").map(str::trim)
}

/// The rule [`every_third_party_action_is_pinned_to_a_commit`] holds
/// each `uses:` value to: a local action, or `owner/repo[/path]@SHA`
/// with the 40-hex commit and a `# <version>` comment naming the ref
/// it was resolved from. A tag or branch can be moved to different
/// code after review; a commit cannot.
fn pinned(value: &str) -> bool {
    if value.starts_with("./") {
        return true;
    }
    let (action, rest) = match value.split_once('@') {
        Some(x) => x,
        None => return false,
    };
    let (sha, comment) = match rest.split_once(char::is_whitespace) {
        Some((s, c)) => (s, c.trim()),
        None => (rest, ""),
    };
    action.split('/').count() >= 2
        && sha.len() == 40
        && sha
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        && comment
            .strip_prefix('#')
            .is_some_and(|v| !v.trim().is_empty())
}

#[test]
fn the_pin_rule_accepts_a_commit_and_refuses_a_movable_ref() {
    let sha = "3d3c42e5aac5ba805825da76410c181273ba90b1";
    assert!(pinned(&format!("actions/checkout@{sha} # v7.0.1")));
    assert!(pinned(&format!(
        "github/codeql-action/init@{sha} # v4.39.1"
    )));
    assert!(pinned("./.github/actions/local"));
    // negative controls: each is a ref that can be moved after review
    assert!(!pinned("actions/checkout@v7"), "a tag");
    assert!(!pinned("dtolnay/rust-toolchain@stable"), "a branch");
    assert!(
        !pinned(&format!("actions/checkout@{sha}")),
        "no version comment"
    );
    assert!(
        !pinned(&format!("actions/checkout@{}", &sha[..39])),
        "39 hex"
    );
    assert!(
        !pinned(&format!("actions/checkout@{} # v7", sha.to_uppercase())),
        "upper"
    );
    assert!(
        !pinned("checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7"),
        "no owner"
    );
}

#[test]
fn every_third_party_action_is_pinned_to_a_commit() {
    let mut bad = Vec::new();
    let mut seen = 0usize;
    for (name, y) in all_workflows() {
        for (n, line) in y.lines().enumerate() {
            if let Some(v) = uses_ref(line) {
                seen += 1;
                if !pinned(v) {
                    bad.push(format!("{name}:{}: {v}", n + 1));
                }
            }
        }
    }
    assert!(seen > 0, "no `uses:` lines were found: the scan is broken");
    assert!(
        bad.is_empty(),
        "not pinned to a commit:\n{}",
        bad.join("\n")
    );
}

/// The value of a top-level `permissions:` key: the block under it, or
/// the inline value after the colon.
fn top_level_permissions(y: &str) -> Option<String> {
    let mut lines = y.lines();
    while let Some(l) = lines.next() {
        if let Some(inline) = l.strip_prefix("permissions:") {
            let inline = inline.trim();
            if !inline.is_empty() {
                return Some(inline.to_string());
            }
            let block: Vec<&str> = lines
                .by_ref()
                .take_while(|l| l.starts_with(' ') || l.trim().is_empty())
                .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
                .map(str::trim)
                .collect();
            return Some(block.join(", "));
        }
    }
    None
}

#[test]
fn every_workflow_starts_from_a_read_only_token() {
    // the default GITHUB_TOKEN can write on a push event; a top-level
    // block caps every job, and the one job that needs write asks for
    // it on its own
    for (name, y) in all_workflows() {
        let p = top_level_permissions(&y)
            .unwrap_or_else(|| panic!("{name}: no top-level `permissions:`"));
        assert!(
            p == "{}" || p == "contents: read",
            "{name}: top-level permissions are `{p}`; grant write per job instead"
        );
    }
    assert_eq!(
        top_level_permissions("on: push\npermissions:\n  contents: write\njobs:\n"),
        Some("contents: write".to_string()),
        "negative control: the reader must see a write grant"
    );
}

#[test]
fn the_release_job_alone_may_write_contents() {
    let y = workflow("release.yml").expect("release.yml");
    let publish = y.split("\n  publish:").nth(1).expect("a `publish` job");
    assert!(
        publish.contains("permissions:") && publish.contains("contents: write"),
        "`gh release create` needs contents: write on the publish job"
    );
    let before = y.split("\n  publish:").next().unwrap();
    let jobs = before.split("\njobs:").nth(1).expect("jobs");
    assert!(
        !jobs.contains("contents: write"),
        "only the publish job writes"
    );
}

#[test]
fn code_scanning_covers_every_language_in_the_repo() {
    let y = workflow("codeql.yml").expect(".github/workflows/codeql.yml");
    // Rust in src/, TypeScript in client/, and the workflows themselves
    for lang in ["rust", "javascript-typescript", "actions"] {
        assert!(
            y.contains(&format!("language: {lang}")),
            "codeql.yml does not analyze {lang}"
        );
    }
    assert!(y.contains("github/codeql-action/init@"));
    assert!(y.contains("github/codeql-action/analyze@"));
    assert!(y.contains("security-events: write"));
    // pushes to main, and a clock: new queries find old code
    assert!(y.contains("branches: [main]") && y.contains("schedule:"));
}

#[test]
fn the_openssf_scorecard_runs_and_reports_to_code_scanning() {
    let y = workflow("scorecard.yml").expect(".github/workflows/scorecard.yml");
    assert!(y.contains("ossf/scorecard-action@"));
    assert!(y.contains("results_format: sarif"));
    assert!(y.contains("github/codeql-action/upload-sarif@"));
    assert!(y.contains("id-token: write") && y.contains("publish_results: true"));
    assert!(y.contains("schedule:") && y.contains("branch_protection_rule:"));
}

#[test]
fn dependabot_watches_cargo_npm_and_actions() {
    let path = format!("{}/.github/dependabot.yml", env!("CARGO_MANIFEST_DIR"));
    let y = std::fs::read_to_string(&path).expect(".github/dependabot.yml");
    for (eco, dir) in [("cargo", "/"), ("npm", "/client"), ("github-actions", "/")] {
        let entry = y
            .split("- package-ecosystem:")
            .find(|s| s.trim_start().starts_with(&format!("\"{eco}\"")))
            .unwrap_or_else(|| panic!("dependabot.yml has no {eco} entry"));
        assert!(
            entry.contains(&format!("directory: \"{dir}\"")),
            "{eco} should watch {dir}"
        );
    }
    // the floor rule: `@types/vscode` is the API surface, not a version
    // to keep current (`vsce package` refuses a range above engines.vscode)
    assert!(y.contains("dependency-name: \"@types/vscode\""));
    // the fuzz package keeps its own Cargo.lock, and ClusterFuzzLite's
    // builder image is pinned by digest: both go stale unless watched
    for (eco, dir) in [("cargo", "/fuzz"), ("docker", "/.clusterfuzzlite")] {
        assert!(
            y.split("- package-ecosystem:").any(|s| {
                s.trim_start().starts_with(&format!("\"{eco}\""))
                    && s.contains(&format!("directory: \"{dir}\""))
            }),
            "dependabot.yml should watch {dir} for {eco}"
        );
    }
}

/// The registry serves each platform build separately. After every
/// target logged "Published", single targets returned 404 for the new
/// version for up to about an hour (0.19.7, 0.21.4, 0.21.5), while the
/// readiness step asked only for the universal build and gave up after
/// 30 minutes. It now waits for every target the `vsix` job packages,
/// each at its own endpoint, for up to 90 minutes.
#[test]
fn openvsx_readiness_waits_for_every_platform_build() {
    let y = workflow("release.yml").expect("release.yml");
    let vsix = y
        .split("\n  vsix:")
        .nth(1)
        .and_then(|s| s.split("\n  openvsx:").next())
        .expect("a vsix job");
    let re = regex::Regex::new(r#""([a-z0-9]+-[a-z0-9]+) (?:x86_64|aarch64) [a-z-]+""#).unwrap();
    let mut targets: Vec<String> = re.captures_iter(vsix).map(|c| c[1].to_string()).collect();
    assert_eq!(targets.len(), 6, "platform builds packaged: {targets:?}");
    targets.push("universal".to_string());

    let ready = y
        .split("Uploaded is not installable")
        .nth(1)
        .and_then(|s| s.split("\n  publish:").next())
        .expect("the readiness step");
    let pending = ready
        .lines()
        .find_map(|l| l.trim().strip_prefix("pending=\""))
        .and_then(|l| l.strip_suffix('"'))
        .expect("a `pending=\"...\"` list of targets");
    let mut listed: Vec<&str> = pending.split_whitespace().collect();
    listed.sort();
    targets.sort();
    assert_eq!(listed, targets, "every packaged target is waited for");
    assert!(
        ready.contains("open-vsx.org/api/$NS/$NAME/$target/$VERSION"),
        "each target is asked for at its own endpoint"
    );
    let rounds: u64 = regex::Regex::new(r"seq 1 (\d+)")
        .unwrap()
        .captures(ready)
        .expect("seq")[1]
        .parse()
        .unwrap();
    let pause: u64 = regex::Regex::new(r"sleep (\d+)")
        .unwrap()
        .captures(ready)
        .expect("sleep")[1]
        .parse()
        .unwrap();
    assert!(
        rounds * pause >= 90 * 60,
        "waits {}s, want 90 min",
        rounds * pause
    );
}

/// The checkouts in one workflow that leave the job's token in
/// `.git/config` without a recorded reason, as `<file>:<line>`.
///
/// `actions/checkout` keeps the token for later git commands unless the
/// step says `persist-credentials: false`, and then every later step in
/// the job, third-party actions included, can read it.  No checkout here
/// needs it: the wiki-sync job pushes the wiki with the token in the
/// push URL, not the checkout's stored credentials, and the release
/// publish job has no checkout and calls `gh` with `GH_TOKEN`.  A
/// checkout that does need the token says `persist-credentials: true`
/// and records why on a comment line in the step:
/// `# keeps the token: <reason>`.
fn checkouts_keeping_the_token(file: &str, yaml: &str) -> Vec<String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let mut bad = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        if !line.contains("uses: actions/checkout@") {
            continue;
        }
        // The step begins at its `- ` line: this one in the `- uses:`
        // form, or the nearest one above in the `- name:` then `uses:`
        // form, where `with:` sits at the same indent as `uses:`.
        let indent_of = |l: &str| l.len() - l.trim_start().len();
        let start = (0..=n)
            .rev()
            .find(|&k| lines[k].trim_start().starts_with("- "))
            .unwrap_or(n);
        let dash = indent_of(lines[start]);
        let step: Vec<&str> = lines[start + 1..]
            .iter()
            .take_while(|l| l.trim().is_empty() || indent_of(l) > dash)
            .map(|l| l.trim())
            .collect();
        let off = step.contains(&"persist-credentials: false");
        let kept_with_reason = step.contains(&"persist-credentials: true")
            && step.iter().any(|l| {
                l.strip_prefix("# keeps the token:")
                    .is_some_and(|why| !why.trim().is_empty())
            });
        if !off && !kept_with_reason {
            bad.push(format!("{file}:{}", n + 1));
        }
    }
    bad
}

#[test]
fn no_checkout_leaves_the_token_without_a_reason() {
    let bad: Vec<String> = all_workflows()
        .iter()
        .flat_map(|(f, y)| checkouts_keeping_the_token(f, y))
        .collect();
    assert!(
        bad.is_empty(),
        "checkout without persist-credentials: false or a recorded reason:\n{}",
        bad.join("\n")
    );
}

#[test]
fn checkouts_keeping_the_token_reads_each_step() {
    let co = "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1\n";
    let next = "      - run: make\n";
    // off
    let y = format!("{co}        with:\n          persist-credentials: false\n{next}");
    assert!(checkouts_keeping_the_token("a.yml", &y).is_empty());
    // not said: the default keeps the token
    let y = format!("{co}{next}");
    assert_eq!(checkouts_keeping_the_token("a.yml", &y), ["a.yml:1"]);
    // the next step's setting does not count for this checkout
    let y = format!("{co}{next}        with:\n          persist-credentials: false\n");
    assert_eq!(checkouts_keeping_the_token("a.yml", &y), ["a.yml:1"]);
    // kept, but with no reason, or an empty one
    let y = format!("{co}        with:\n          persist-credentials: true\n{next}");
    assert_eq!(checkouts_keeping_the_token("a.yml", &y), ["a.yml:1"]);
    let y = format!(
        "{co}        # keeps the token:\n        with:\n          persist-credentials: true\n{next}"
    );
    assert_eq!(checkouts_keeping_the_token("a.yml", &y), ["a.yml:1"]);
    // the `- name:` then `uses:` form, with `with:` beside `uses:`
    let named = "      - name: Checkout\n        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7\n";
    let y = format!("{named}        with:\n          persist-credentials: false\n{next}");
    assert!(checkouts_keeping_the_token("a.yml", &y).is_empty());
    let y = format!("{named}{next}        with:\n          persist-credentials: false\n");
    assert_eq!(checkouts_keeping_the_token("a.yml", &y), ["a.yml:2"]);
    // kept, with its reason
    let y = format!(
        "{co}        # keeps the token: pushes the tag\n        with:\n          persist-credentials: true\n{next}"
    );
    assert!(checkouts_keeping_the_token("a.yml", &y).is_empty());
}

/// actionlint checks every workflow, shell scripts included through
/// shellcheck, on every push. actionlint 1.7.12 found 13 places in
/// release.yml where an unquoted expansion would be split or globbed,
/// and one in the grammar sanitizer step of ci.yml; a gate that reads
/// the workflows as text cannot see those. The binary is pinned by
/// version and sha256 so the check is the same on every run, and
/// shellcheck must be present, because without it actionlint skips the
/// script checks and passes.
#[test]
fn ci_runs_a_pinned_actionlint_over_every_workflow() {
    let y = workflow("ci.yml").expect("ci.yml must exist");
    let (_, job) = y
        .split_once("\n  actionlint:")
        .expect("ci.yml has an `actionlint` job");
    let job = job.split("\n  ci-success:").next().unwrap_or(job);
    assert!(
        job.contains("actionlint_1.7.12_linux_amd64.tar.gz"),
        "actionlint is pinned to a release artefact"
    );
    let sha = regex::Regex::new(r"[0-9a-f]{64}  actionlint_1\.7\.12_linux_amd64\.tar\.gz").unwrap();
    assert!(
        sha.is_match(job) && job.contains("sha256sum -c"),
        "the artefact's sha256 is pinned and checked"
    );
    assert!(
        job.contains("command -v shellcheck"),
        "the job fails when shellcheck is missing"
    );
    assert!(
        job.contains("./actionlint -color") || job.contains("./actionlint\n"),
        "actionlint runs with no file list, so it reads every workflow"
    );
    let needs = y
        .split_once("\n  ci-success:")
        .and_then(|(_, rest)| rest.lines().find(|l| l.trim_start().starts_with("needs:")))
        .expect("ci-success has needs");
    assert!(
        needs.contains("actionlint"),
        "CI success waits for actionlint: {needs}"
    );
}

/// The tag's verify job compares every file that records the release
/// version. `fuzz/Cargo.lock` records the server's own version (the fuzz
/// crate depends on it by path); 0.21.6 was released with it still
/// saying 0.21.5, because nothing compared it.
#[test]
fn the_release_verify_job_reads_the_fuzz_lockfile() {
    let y = workflow("release.yml").expect("release.yml");
    let verify = y
        .split("\n  verify:")
        .nth(1)
        .and_then(|s| s.split("\n  server:").next())
        .expect("a verify job");
    assert!(
        verify.contains("fuzz/Cargo.lock"),
        "the verify job does not compare fuzz/Cargo.lock's kamailio-lsp version with the tag"
    );
}
