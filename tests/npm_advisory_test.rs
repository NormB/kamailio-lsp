//! Every npm package version in the repository that a known advisory
//! covers is either absent, or recorded as not affecting kamailio-lsp
//! with the facts that make it so, and those facts still hold.
//!
//! The VS Code client ships its runtime `node_modules` inside the VSIX, so
//! a vulnerable version in `client/package-lock.json` reaches every user.
//! [`ADVISORIES`] names each advisory, the package and the first fixed
//! version. When no fixed version can be used, an `[[IgnoredVulns]]` entry
//! in the `osv-scanner.toml` next to the lockfile records the decision
//! (OSV-Scanner and the OpenSSF Scorecard read it), and [`NOT_AFFECTED`]
//! names the only package allowed to pull the affected one.
//!
//! The current record, GHSA-g3hv-75wg-m249 in minimatch, holds because
//! minimatch is reached only through vscode-languageclient 11.0.0, which
//! calls it on two paths: file-operation globs the server registers, and
//! the `pattern` of a document or notebook filter. The rules here keep
//! both closed: the server's file-operation globs and filter patterns
//! must be string literals and it advertises no notebook sync, and the
//! client's document selector has no pattern.
//!
//! Each rule is a function of the text it reads, so the tests at the end
//! drive it with text that breaks the rule as well as the real files.

use std::collections::BTreeSet;
use std::path::Path;

/// A package, the first version that is not affected, and the advisory.
struct Advisory {
    package: &'static str,
    fixed: (u64, u64, u64),
    id: &'static str,
}

/// The advisories the lockfiles are checked against.
const ADVISORIES: &[Advisory] = &[
    // minimatch pattern amplification in firstPhasePreProcess(): every
    // release before 11.0.1 (https://osv.dev/GHSA-g3hv-75wg-m249).
    Advisory {
        package: "minimatch",
        fixed: (11, 0, 1),
        id: "GHSA-g3hv-75wg-m249",
    },
];

/// An advisory recorded as not affecting kamailio-lsp, and the one
/// package, at the one version the analysis read, allowed to pull the
/// affected package.
struct NotAffected {
    id: &'static str,
    via: &'static str,
    via_version: &'static str,
}

/// The not-affected records the `osv-scanner.toml` files may hold.
const NOT_AFFECTED: &[NotAffected] = &[NotAffected {
    id: "GHSA-g3hv-75wg-m249",
    via: "vscode-languageclient",
    via_version: "11.0.0",
}];

/// The lockfile names this test reads.
const NPM_LOCKFILES: &[&str] = &["package-lock.json", "npm-shrinkwrap.json"];

/// Lockfile names of other JavaScript package managers, which this test
/// cannot read. One appearing must extend the test, not slip past it.
const UNREAD_LOCKFILES: &[&str] = &["yarn.lock", "pnpm-lock.yaml", "bun.lockb", "bun.lock"];

/// `major.minor.patch` and whether a prerelease tag follows, or `None`
/// when the text is not a version.
fn parse_version(v: &str) -> Option<((u64, u64, u64), bool)> {
    let (core, pre) = match v.split_once(['-', '+']) {
        Some((core, rest)) => (core, v[core.len()..].starts_with('-') && !rest.is_empty()),
        None => (v, false),
    };
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(((major, minor, patch), pre))
}

/// Whether `version` comes before `fixed`. A prerelease of `fixed` comes
/// before it, and text that is not a version counts as affected.
fn is_affected(version: &str, fixed: (u64, u64, u64)) -> bool {
    match parse_version(version) {
        Some((core, pre)) => core < fixed || (core == fixed && pre),
        None => true,
    }
}

/// The package an entry of a lockfile's `packages` map installs: its
/// `name` field when it has one (an npm alias), else the last
/// `node_modules/` segment of its key. The root entry (`""`) and linked
/// workspace folders install no registry package.
fn entry_package<'a>(key: &'a str, entry: &'a serde_json::Value) -> Option<&'a str> {
    let (_, last) = key.rsplit_once("node_modules/")?;
    Some(
        entry
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(last),
    )
}

/// One package a lockfile installs.
struct Installed {
    key: String,
    package: String,
    version: String,
    /// The names in its `dependencies`, `optionalDependencies` and
    /// `peerDependencies`.
    requires: BTreeSet<String>,
}

/// Every package an npm lockfile installs. Panics on a lockfile it cannot
/// read in full, so an unreadable file fails instead of passing empty.
fn installed(lock: &str) -> Vec<Installed> {
    let json: serde_json::Value = serde_json::from_str(lock).expect("the lockfile is JSON");
    let lockfile_version = json
        .get("lockfileVersion")
        .and_then(serde_json::Value::as_u64)
        .expect("the lockfile has a lockfileVersion");
    assert!(
        lockfile_version >= 2,
        "lockfileVersion {lockfile_version} has no `packages` map; regenerate it with npm 7 or later"
    );
    let packages = json
        .get("packages")
        .and_then(serde_json::Value::as_object)
        .expect("the lockfile has a `packages` map");
    let mut out = Vec::new();
    for (key, entry) in packages {
        let Some(package) = entry_package(key, entry) else {
            continue;
        };
        let version = entry
            .get("version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{key} has no version"));
        let requires = ["dependencies", "optionalDependencies", "peerDependencies"]
            .iter()
            .filter_map(|field| entry.get(field).and_then(serde_json::Value::as_object))
            .flat_map(|deps| deps.keys().cloned())
            .collect();
        out.push(Installed {
            key: key.clone(),
            package: package.to_string(),
            version: version.to_string(),
            requires,
        });
    }
    out
}

/// One `[[IgnoredVulns]]` entry of an `osv-scanner.toml`.
#[derive(Debug, Default, PartialEq)]
struct Ignore {
    id: String,
    reason: Option<String>,
    /// `ignoreUntil` as `(year, month, day)`.
    until: Option<(u32, u32, u32)>,
}

/// The `[[IgnoredVulns]]` entries of an `osv-scanner.toml`. Enough TOML
/// for this file: comments, `[[IgnoredVulns]]` headers, basic strings on
/// one line and offset date-times. Anything else panics, so a file this
/// cannot read fails instead of passing empty.
fn parse_ignores(text: &str) -> Vec<Ignore> {
    let mut out: Vec<Ignore> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[IgnoredVulns]]" {
            out.push(Ignore::default());
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .map(|(k, v)| (k.trim(), v.trim()))
            .unwrap_or_else(|| panic!("line {}: not `key = value`: {raw}", n + 1));
        let entry = out
            .last_mut()
            .unwrap_or_else(|| panic!("line {}: {key} before any [[IgnoredVulns]]", n + 1));
        match key {
            "id" => entry.id = basic_string(value, n),
            "reason" => entry.reason = Some(basic_string(value, n)),
            "ignoreUntil" => entry.until = Some(date_prefix(value, n)),
            _ => panic!("line {}: key {key} is not read by this test", n + 1),
        }
    }
    out
}

/// The text of a one-line TOML basic string with no escapes.
fn basic_string(value: &str, n: usize) -> String {
    let inner = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or_else(|| panic!("line {}: not a one-line string: {value}", n + 1));
    assert!(
        !inner.contains(['"', '\\']),
        "line {}: escapes are not read by this test",
        n + 1
    );
    inner.to_string()
}

/// The `(year, month, day)` a TOML date or date-time starts with.
fn date_prefix(value: &str, n: usize) -> (u32, u32, u32) {
    let date = value.get(..10).unwrap_or(value);
    let parts: Vec<u32> = date
        .split('-')
        .map(|p| {
            p.parse()
                .unwrap_or_else(|_| panic!("line {}: not a date: {value}", n + 1))
        })
        .collect();
    match parts[..] {
        [y, m, d] if (1..=12).contains(&m) && (1..=31).contains(&d) => (y, m, d),
        _ => panic!("line {}: not a date: {value}", n + 1),
    }
}

/// The civil date (UTC) `days` days after 1970-01-01.
fn civil_from_days(days: i64) -> (u32, u32, u32) {
    // Howard Hinnant's days_from_civil, inverted.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let out = |v: i64| u32::try_from(v).expect("a date after 1970 fits u32");
    (out(y), out(m), out(d))
}

/// Today's date, UTC.
fn today() -> (u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs();
    civil_from_days(i64::try_from(secs / 86_400).expect("days fit i64"))
}

/// What is wrong with one lockfile and the ignore entries next to it, on
/// `today`. Empty when every advised version is recorded as not
/// affecting the project, each record is current and justified, only the
/// analyzed parent pulls the package, and no record outlives its version.
fn lockfile_problems(
    lock: &str,
    ignores: &[Ignore],
    advisories: &[Advisory],
    not_affected: &[NotAffected],
    today: (u32, u32, u32),
) -> Vec<String> {
    let installed = installed(lock);
    let mut problems = Vec::new();
    for ignore in ignores {
        if !not_affected.iter().any(|r| r.id == ignore.id) {
            problems.push(format!(
                "{} is ignored without a not-affected record in this test",
                ignore.id
            ));
        }
        if ignore.reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
            problems.push(format!("{} is ignored without a reason", ignore.id));
        }
        match ignore.until {
            None => problems.push(format!("{} is ignored without an ignoreUntil", ignore.id)),
            Some(until) if until <= today => problems.push(format!(
                "{} was ignored until {until:?}, which has passed; analyze it again",
                ignore.id
            )),
            Some(_) => {}
        }
    }
    for advisory in advisories {
        let affected: Vec<&Installed> = installed
            .iter()
            .filter(|i| i.package == advisory.package && is_affected(&i.version, advisory.fixed))
            .collect();
        let ignored = ignores.iter().any(|i| i.id == advisory.id);
        if affected.is_empty() {
            if ignored {
                problems.push(format!(
                    "{} no longer resolves an affected {}; remove its ignore entry",
                    advisory.id, advisory.package
                ));
            }
            continue;
        }
        if !ignored {
            for i in &affected {
                problems.push(format!(
                    "{} resolves {} {}, which {} covers, with no ignore entry",
                    i.key, i.package, i.version, advisory.id
                ));
            }
            continue;
        }
        let Some(record) = not_affected.iter().find(|r| r.id == advisory.id) else {
            continue; // reported above
        };
        for parent in installed
            .iter()
            .filter(|i| i.requires.contains(advisory.package))
        {
            if parent.package != record.via || parent.version != record.via_version {
                problems.push(format!(
                    "{} {} pulls {}; the not-affected record for {} covers only {} {}",
                    parent.package,
                    parent.version,
                    advisory.package,
                    advisory.id,
                    record.via,
                    record.via_version
                ));
            }
        }
    }
    problems
}

/// The struct literals named `ty` in Rust source, from the name to the
/// matching close brace.
fn struct_literals<'a>(src: &'a str, ty: &str) -> Vec<&'a str> {
    let open = format!("{ty} {{");
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find(&open) {
        let body = &rest[at..];
        // `DocumentFilter {` inside `TextDocumentFilter {` is not a match.
        if rest[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            rest = &body[open.len()..];
            continue;
        }
        let mut depth = 0usize;
        let mut end = body.len();
        for (i, c) in body.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(&body[..end]);
        rest = &body[end..];
    }
    out
}

/// Whether the field `field` in a struct literal is a string literal:
/// `"…"`, optionally followed by `.into()`, `.to_string()` or `.to_owned()`,
/// or `String::from("…")`. For an `Option` field, `None` and `Some(` that
/// literal also pass.
fn field_is_literal(literal: &str, field: &str) -> bool {
    /// The expression a field's value starts with: up to the first comma
    /// or closing bracket outside strings and brackets.
    fn field_value(text: &str) -> &str {
        let text = text.trim_start();
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for (i, c) in text.char_indices() {
            if in_string {
                match c {
                    _ if escaped => escaped = false,
                    '\\' => escaped = true,
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth == 0 => return text[..i].trim(),
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => return text[..i].trim(),
                _ => {}
            }
        }
        text.trim()
    }

    let Some(at) = literal.find(&format!("{field}:")) else {
        return true; // a missing field is the type's default
    };
    let value = field_value(&literal[at + field.len() + 1..]);
    let value = value
        .strip_prefix("Some(")
        .and_then(|v| v.strip_suffix(')'))
        .unwrap_or(value);
    if value == "None" {
        return true;
    }
    let value = value
        .strip_prefix("String::from(")
        .and_then(|v| v.strip_suffix(')'))
        .unwrap_or(value);
    let value = [".into()", ".to_string()", ".to_owned()"]
        .iter()
        .find_map(|s| value.strip_suffix(s))
        .unwrap_or(value);
    value.len() >= 2 && value.starts_with('"') && value.ends_with('"')
}

/// What in the server's source could hand vscode-languageclient a glob
/// it compiles with minimatch: a file-operation glob or a document filter
/// pattern that is not a string literal, or notebook sync, whose filters
/// it matches the same way.
fn server_glob_problems(file: &str, src: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for literal in struct_literals(src, "FileOperationPattern") {
        if !field_is_literal(literal, "glob") {
            problems.push(format!(
                "{file}: a file-operation glob is not a string literal: {literal}"
            ));
        }
    }
    for ty in ["DocumentFilter", "TextDocumentFilter"] {
        for literal in struct_literals(src, ty) {
            if !field_is_literal(literal, "pattern") {
                problems.push(format!(
                    "{file}: a document filter pattern is not a string literal: {literal}"
                ));
            }
        }
    }
    for (n, line) in src.lines().enumerate() {
        if let Some(at) = line.find("notebook_document_sync:")
            && line[at + "notebook_document_sync:".len()..].trim() != "None,"
        {
            problems.push(format!("{file}:{}: advertises notebook sync", n + 1));
        }
    }
    problems
}

/// Whether a client `documentSelector` in TypeScript source has a
/// `pattern`, which vscode-languageclient compiles with minimatch.
fn client_selector_problems(file: &str, src: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find("documentSelector") {
        let after = &rest[at..];
        let end = after.find(']').map_or(after.len(), |e| e + 1);
        let selector = &after[..end];
        if selector.contains("pattern") {
            problems.push(format!(
                "{file}: the document selector has a pattern: {selector}"
            ));
        }
        rest = &after[end..];
    }
    problems
}

/// The git-tracked files `keep` accepts.
fn tracked(root: &Path, keep: impl Fn(&str) -> bool) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|p| !p.is_empty() && keep(p))
        .map(str::to_string)
        .collect()
}

/// The file name of a repository path.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn read(root: &Path, rel: &str) -> String {
    let path = root.join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()))
}

#[test]
fn every_advised_npm_version_has_a_current_not_affected_record() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let locks = tracked(root, |p| NPM_LOCKFILES.contains(&file_name(p)));
    assert!(
        locks.iter().any(|l| l == "client/package-lock.json"),
        "client/package-lock.json is not tracked; found {locks:?}"
    );
    let today = today();
    let mut problems = Vec::new();
    for lock in &locks {
        let dir = lock.rsplit_once('/').map_or("", |(d, _)| d);
        let config = if dir.is_empty() {
            "osv-scanner.toml".to_string()
        } else {
            format!("{dir}/osv-scanner.toml")
        };
        let ignores = if root.join(&config).exists() {
            parse_ignores(&read(root, &config))
        } else {
            Vec::new()
        };
        problems.extend(
            lockfile_problems(&read(root, lock), &ignores, ADVISORIES, NOT_AFFECTED, today)
                .into_iter()
                .map(|p| format!("{lock}: {p}")),
        );
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn every_osv_scanner_config_sits_next_to_a_lockfile() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let locks = tracked(root, |p| NPM_LOCKFILES.contains(&file_name(p)));
    for config in tracked(root, |p| file_name(p) == "osv-scanner.toml") {
        let dir = config.rsplit_once('/').map_or("", |(d, _)| d);
        assert!(
            locks
                .iter()
                .any(|l| l.rsplit_once('/').map_or("", |(d, _)| d) == dir),
            "{config} sits next to no npm lockfile, so this test never reads it"
        );
    }
}

#[test]
fn the_server_hands_the_client_no_glob_it_compiles_with_minimatch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = tracked(root, |p| p.starts_with("src/") && p.ends_with(".rs"));
    assert!(
        files.iter().any(|f| f == "src/server.rs"),
        "src/server.rs is not tracked"
    );
    let problems: Vec<String> = files
        .iter()
        .flat_map(|f| server_glob_problems(f, &read(root, f)))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_client_document_selector_has_no_pattern() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = tracked(root, |p| p.starts_with("client/src/") && p.ends_with(".ts"));
    let sources: Vec<(String, String)> = files.iter().map(|f| (f.clone(), read(root, f))).collect();
    assert!(
        sources.iter().any(|(_, s)| s.contains("documentSelector")),
        "no client source sets a documentSelector; the check reads nothing"
    );
    let problems: Vec<String> = sources
        .iter()
        .flat_map(|(f, s)| client_selector_problems(f, s))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn every_javascript_lockfile_is_one_this_test_reads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let unread = tracked(root, |p| UNREAD_LOCKFILES.contains(&file_name(p)));
    assert!(
        unread.is_empty(),
        "{unread:?} cannot be checked for advised versions; extend this test to read them"
    );
}

// The rules on text that breaks them. A rule that cannot fail proves
// nothing.

/// A lockfile with one `packages` entry per `(key, body)` pair.
fn lockfile(entries: &[(&str, &str)]) -> String {
    let body: Vec<String> = entries
        .iter()
        .map(|(key, body)| format!("\"{key}\": {body}"))
        .collect();
    format!(
        "{{\"lockfileVersion\": 3, \"packages\": {{\"\": {{\"name\": \"x\"}}, {}}}}}",
        body.join(", ")
    )
}

/// The client's shape: vscode-languageclient 11.0.0 pulling `minimatch`.
fn client_lock(minimatch: &str) -> String {
    lockfile(&[
        (
            "node_modules/vscode-languageclient",
            "{\"version\": \"11.0.0\", \"dependencies\": {\"minimatch\": \"^10.2.6\"}}",
        ),
        (
            "node_modules/minimatch",
            &format!("{{\"version\": \"{minimatch}\"}}"),
        ),
    ])
}

const DAY: (u32, u32, u32) = (2026, 11, 3);

fn ignore(until: (u32, u32, u32)) -> Ignore {
    Ignore {
        id: "GHSA-g3hv-75wg-m249".into(),
        reason: Some("not reached".into()),
        until: Some(until),
    }
}

fn problems(lock: &str, ignores: &[Ignore]) -> Vec<String> {
    lockfile_problems(lock, ignores, ADVISORIES, NOT_AFFECTED, DAY)
}

#[test]
fn an_affected_version_without_an_ignore_entry_fails() {
    for version in ["10.2.6", "3.1.2", "11.0.0", "11.0.1-rc.1", "not-a-version"] {
        let found = problems(&client_lock(version), &[]);
        assert_eq!(found.len(), 1, "{version}: {found:?}");
        assert!(found[0].contains("with no ignore entry"), "{found:?}");
    }
}

#[test]
fn a_current_justified_record_passes() {
    assert_eq!(
        problems(&client_lock("10.2.6"), &[ignore((2027, 2, 1))]),
        Vec::<String>::new()
    );
}

#[test]
fn an_expired_or_unjustified_record_fails() {
    let expired = problems(&client_lock("10.2.6"), &[ignore(DAY)]);
    assert!(
        expired.len() == 1 && expired[0].contains("has passed"),
        "{expired:?}"
    );
    for bad in [
        Ignore {
            reason: None,
            ..ignore((2027, 2, 1))
        },
        Ignore {
            reason: Some("  ".into()),
            ..ignore((2027, 2, 1))
        },
    ] {
        let found = problems(&client_lock("10.2.6"), &[bad]);
        assert!(
            found.len() == 1 && found[0].contains("without a reason"),
            "{found:?}"
        );
    }
    let open_ended = problems(
        &client_lock("10.2.6"),
        &[Ignore {
            until: None,
            ..ignore((2027, 2, 1))
        }],
    );
    assert!(
        open_ended.len() == 1 && open_ended[0].contains("without an ignoreUntil"),
        "{open_ended:?}"
    );
}

#[test]
fn a_record_that_outlives_the_affected_version_fails() {
    for version in ["11.0.1", "11.0.4", "12.0.0", "11.0.1+build.5"] {
        assert_eq!(problems(&client_lock(version), &[]), Vec::<String>::new());
        let found = problems(&client_lock(version), &[ignore((2027, 2, 1))]);
        assert!(
            found.len() == 1 && found[0].contains("remove its ignore entry"),
            "{version}: {found:?}"
        );
    }
}

#[test]
fn another_parent_of_the_affected_package_fails() {
    let lock = lockfile(&[
        (
            "node_modules/vscode-languageclient",
            "{\"version\": \"11.0.0\", \"dependencies\": {\"minimatch\": \"^10.2.6\"}}",
        ),
        (
            "node_modules/glob",
            "{\"version\": \"10.4.5\", \"optionalDependencies\": {\"minimatch\": \"^9.0.4\"}}",
        ),
        ("node_modules/minimatch", "{\"version\": \"10.2.6\"}"),
    ]);
    let found = problems(&lock, &[ignore((2027, 2, 1))]);
    assert!(
        found.len() == 1 && found[0].starts_with("glob 10.4.5 pulls minimatch"),
        "{found:?}"
    );
    let bumped = lockfile(&[
        (
            "node_modules/vscode-languageclient",
            "{\"version\": \"11.1.0\", \"dependencies\": {\"minimatch\": \"^10.2.6\"}}",
        ),
        ("node_modules/minimatch", "{\"version\": \"10.2.6\"}"),
    ]);
    let found = problems(&bumped, &[ignore((2027, 2, 1))]);
    assert!(
        found.len() == 1 && found[0].starts_with("vscode-languageclient 11.1.0 pulls"),
        "a parent version the analysis did not read: {found:?}"
    );
}

#[test]
fn an_ignore_without_a_record_in_this_test_fails() {
    let found = problems(
        &client_lock("11.0.4"),
        &[Ignore {
            id: "GHSA-0000-0000-0000".into(),
            ..ignore((2027, 2, 1))
        }],
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("without a not-affected record"),
        "{found:?}"
    );
}

#[test]
fn nested_and_aliased_copies_are_found() {
    let lock = lockfile(&[
        ("node_modules/minimatch", "{\"version\": \"11.0.4\"}"),
        (
            "node_modules/a/node_modules/minimatch",
            "{\"version\": \"9.0.9\"}",
        ),
        (
            "node_modules/mm",
            "{\"name\": \"minimatch\", \"version\": \"5.1.9\"}",
        ),
        ("node_modules/minimatch-like", "{\"version\": \"1.0.0\"}"),
    ]);
    let found = problems(&lock, &[]);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].starts_with("node_modules/a/node_modules/minimatch resolves"));
    assert!(found[1].starts_with("node_modules/mm resolves"));
}

#[test]
#[should_panic(expected = "has no `packages` map")]
fn a_lockfile_without_a_packages_map_is_refused() {
    installed("{\"lockfileVersion\": 1, \"dependencies\": {}}");
}

#[test]
fn the_ignore_file_reader_reads_what_osv_scanner_reads() {
    let text = "# note\n[[IgnoredVulns]]\nid = \"GHSA-a\"\nignoreUntil = 2027-02-01T00:00:00Z\nreason = \"r\"\n\n[[IgnoredVulns]]\nid = \"GHSA-b\"\n";
    assert_eq!(
        parse_ignores(text),
        vec![
            Ignore {
                id: "GHSA-a".into(),
                reason: Some("r".into()),
                until: Some((2027, 2, 1)),
            },
            Ignore {
                id: "GHSA-b".into(),
                ..Ignore::default()
            },
        ]
    );
}

#[test]
#[should_panic(expected = "is not read by this test")]
fn the_ignore_file_reader_refuses_keys_it_does_not_know() {
    parse_ignores("[[IgnoredVulns]]\nid = \"GHSA-a\"\neffectiveUntil = 2027-02-01\n");
}

#[test]
fn the_calendar_matches_known_dates() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(20_760), (2026, 11, 3));
    assert_eq!(civil_from_days(11_016), (2000, 2, 29));
}

#[test]
fn the_server_rule_flags_computed_globs_and_notebook_sync() {
    let good = "FileOperationPattern { glob: \"**/*.{cfg,inc}\".into(), matches: None, options: None }\n\
                DocumentFilter { language: None, scheme: None, pattern: Some(\"**/*.cfg\".into()) }\n\
                notebook_document_sync: None,\n";
    assert_eq!(server_glob_problems("s.rs", good), Vec::<String>::new());
    let bad = "FileOperationPattern {\n    glob: format!(\"{dir}/**\"),\n    matches: None,\n}\n\
               DocumentFilter { pattern: Some(user_glob.clone()), language: None }\n\
               notebook_document_sync: Some(sync),\n";
    let found = server_glob_problems("s.rs", bad);
    assert_eq!(found.len(), 3, "{found:#?}");
    assert!(found[0].contains("file-operation glob"));
    assert!(found[1].contains("document filter pattern"));
    assert!(found[2].contains("notebook sync"));
}

#[test]
fn the_client_rule_flags_a_selector_pattern() {
    assert_eq!(
        client_selector_problems("e.ts", "documentSelector: [{ language: 'kamailio-cfg' }],"),
        Vec::<String>::new()
    );
    let found = client_selector_problems(
        "e.ts",
        "documentSelector: [{ language: 'kamailio-cfg' }, { pattern: '**/*.cfg' }],",
    );
    assert_eq!(found.len(), 1, "{found:?}");
}
