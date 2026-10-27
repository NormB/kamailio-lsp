//! How to report a bug, ask for a feature and report a vulnerability is
//! written down where a newcomer looks for it: `CONTRIBUTING.md` names the
//! issue tracker and the private vulnerability form, and `README.md` links
//! to that section.  The issue forms in `.github/ISSUE_TEMPLATE/` exist,
//! but nothing outside GitHub's "New issue" page said so.

const REPO: &str = "https://github.com/NormB/kamailio-lsp";

fn read(path: &str) -> String {
    std::fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// The text of the `## Reporting bugs` section of `CONTRIBUTING.md`.
fn reporting_section() -> String {
    let doc = read("CONTRIBUTING.md");
    let (_, rest) = doc
        .split_once("\n## Reporting bugs\n")
        .expect("CONTRIBUTING.md has no `## Reporting bugs` section");
    rest.split("\n## ").next().unwrap_or("").to_string()
}

#[test]
fn contributing_names_the_issue_forms_and_the_private_vulnerability_form() {
    let section = reporting_section();
    for needle in [
        format!("{REPO}/issues/new/choose"),
        format!("{REPO}/issues"),
        format!("{REPO}/security/advisories/new"),
        "SECURITY.md".to_string(),
    ] {
        assert!(
            section.contains(&needle),
            "the Reporting bugs section does not name {needle}"
        );
    }
}

#[test]
fn the_issue_forms_it_names_exist() {
    for form in ["bug_report.yml", "feature_request.yml"] {
        let text = read(&format!(".github/ISSUE_TEMPLATE/{form}"));
        assert!(!text.trim().is_empty(), "{form} is empty");
    }
}

#[test]
fn readme_links_to_the_reporting_section() {
    let readme = read("README.md");
    let link = format!("{REPO}/blob/main/CONTRIBUTING.md#reporting-bugs");
    assert!(readme.contains(&link), "README.md does not link {link}");
}

/// The "New issue" page offered a "Question / discussion" link to the
/// repository's Discussions, which are turned off here, so the link
/// answered 404.  If Discussions are turned on, the link can return with
/// this test.
#[test]
fn the_new_issue_page_links_no_discussions() {
    let config = read(".github/ISSUE_TEMPLATE/config.yml");
    assert!(
        !config.contains(&format!("{REPO}/discussions")),
        "config.yml links the Discussions page, which this repository does not have"
    );
}
