//! Marketplace-asset gates: the extension icon must be a valid
//! 256x256 PNG with an alpha channel (the marketplace renders icons
//! on light and dark surfaces), and the manifest must reference it.

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

#[test]
fn icon_is_a_256x256_rgba_png() {
    let root = env!("CARGO_MANIFEST_DIR");
    let png = std::fs::read(format!("{root}/client/icon.png")).expect("client/icon.png missing");
    assert!(png.len() > 33, "truncated png");
    assert_eq!(
        &png[..8],
        &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'],
        "not a PNG signature"
    );
    // the first chunk is IHDR: length(4) type(4) width(4) height(4)
    // bitdepth(1) colortype(1)
    assert_eq!(&png[12..16], b"IHDR");
    let (w, h) = (be32(&png[16..20]), be32(&png[20..24]));
    assert_eq!((w, h), (256, 256), "icon must be 256x256, got {w}x{h}");
    let color_type = png[25];
    assert_eq!(
        color_type, 6,
        "icon must carry an alpha channel (RGBA color type 6), got {color_type}"
    );
}

#[test]
fn manifest_references_the_icon() {
    let root = env!("CARGO_MANIFEST_DIR");
    let manifest = std::fs::read_to_string(format!("{root}/client/package.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(v["icon"], "icon.png");
    assert_eq!(v["publisher"], "NormB");
    assert_eq!(v["displayName"], "Kamailio Routing Script");
    // both installed side by side: kamailio-lsp must NOT claim the
    // bare .cfg extension (opensips-lsp already does)
    let lang = &v["contributes"]["languages"][0];
    assert!(
        lang.get("extensions").is_none(),
        "must not claim *.cfg: {lang}"
    );
    assert_eq!(lang["filenames"][0], "kamailio.cfg");

    // A first-line marker must be one the real lexer defines.  The
    // script types are `#!SER`, `#!KAMAILIO`|`#!OPENSER` and
    // `#!MAXCOMPAT`|`#!ALL` (src/core/cfg.lex): claiming a file on a
    // marker the parser has never heard of would be inventing a rule.
    let first = lang["firstLine"].as_str().expect("a first-line rule");
    assert!(
        first.starts_with("^#!"),
        "must anchor on the marker: {first}"
    );
    for marker in ["KAMAILIO", "OPENSER", "SER", "MAXCOMPAT", "ALL"] {
        assert!(
            first.contains(marker),
            "{marker} is a script type cfg.lex accepts; the rule omits it"
        );
    }

    // Every pattern the extension claims must be documented where a
    // user will look.  Widening the association silently is how you
    // hijack someone's file with no explanation on the page they
    // read — and understating it, as the v0.11.3 note did by saying
    // only `kamailio.cfg` was claimed, sends people to configure
    // something that already worked.
    // Both pages count.  The marketplace listing is what a prospective
    // user reads; the getting-started guide is where someone whose
    // file has no colours actually goes, and its "No colors" row named
    // only `kamailio.cfg` long after the patterns had widened.
    let pages = [
        (
            "client/README.md",
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/client/README.md"))
                .unwrap(),
        ),
        (
            "docs/GETTING_STARTED.md",
            std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/GETTING_STARTED.md"
            ))
            .unwrap(),
        ),
    ];
    let patterns = lang["filenamePatterns"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for p in &patterns {
        let p = p.as_str().unwrap();
        assert!(!p.starts_with("*.cfg") && p != "*", "{p} claims every .cfg");
        for (name, text) in &pages {
            assert!(
                text.contains(p),
                "{name} does not tell the reader that {p} is claimed"
            );
        }
    }
    // the marker set is the lexer's, so the pages must carry all of it
    // rather than the one everybody remembers
    for (name, text) in &pages {
        for marker in ["KAMAILIO", "OPENSER", "SER", "MAXCOMPAT", "ALL"] {
            assert!(
                text.contains(&format!("#!{marker}")),
                "{name} does not say #!{marker} is honoured"
            );
        }
    }
}

#[test]
fn textmate_grammar_directive_set_matches_the_lexer() {
    // cfg.lex PREP_START is "#!" | "!!" (not line-anchored) and the
    // directive names are fixed; the TextMate grammar must highlight
    // the real set, no inventions
    let root = env!("CARGO_MANIFEST_DIR");
    let g = std::fs::read_to_string(format!("{root}/client/syntaxes/kamailio.tmLanguage.json"))
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&g).unwrap();
    let pattern = v["patterns"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|p| {
            let m = p["match"].as_str()?;
            m.contains("ifdef").then(|| m.to_string())
        })
        .expect("directive pattern present");
    for d in [
        "KAMAILIO",
        "define",
        "def",
        "ifdef",
        "ifndef",
        "ifexp",
        "else",
        "endif",
        "trydefine",
        "trydef",
        "redefine",
        "redef",
        "subst",
        "substdef",
        "substdefs",
        "defexp",
        "defexps",
        "defenv",
        "defenvs",
        "trydefenv",
        "trydefenvs",
        "include_file",
        "import_file",
    ] {
        assert!(
            pattern.contains(d),
            "directive '{d}' missing from the TextMate pattern"
        );
    }
    assert!(
        !pattern.contains("defval"),
        "defval is not a kamailio directive"
    );
    assert!(
        !pattern.starts_with("^"),
        "directives are not line-anchored (indentation is legal)"
    );
    assert!(
        pattern.contains("!!"),
        "the '!!' PREP_START spelling must be highlighted"
    );
}

/// An included fragment is rarely named so a filename pattern could
/// catch it, and claiming `*.cfg` outright would hijack every
/// unrelated config file in the workspace — the one thing the
/// association above deliberately refuses to do.  What is left is to
/// ask the server, which has read the configuration that says what
/// includes what, and set the language at runtime.
///
/// Two things must hold for that to ever happen and neither is
/// visible to `tsc` or to `cargo`:
///
///   * the extension has to be RUNNING in a workspace whose only
///     kamailio file on screen is an unassociated fragment, so
///     activation cannot rest on the language association alone;
///   * the method it asks with has to be the method the server
///     registers.  A rename on either side compiles cleanly in both
///     languages and fails only at runtime, in the one situation this
///     feature exists for.
#[test]
fn an_included_fragment_is_associated_at_runtime() {
    let root = env!("CARGO_MANIFEST_DIR");
    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{root}/client/package.json")).unwrap(),
    )
    .unwrap();
    let ext = std::fs::read_to_string(format!("{root}/client/src/extension.ts")).unwrap();
    let main = std::fs::read_to_string(format!("{root}/src/main.rs")).unwrap();

    // the name is taken from the server's own registration, so
    // retiring the request retires this requirement instead of
    // leaving a gate demanding a dead method
    let method = main
        .split("custom_method(")
        .nth(1)
        .and_then(|t| t.split('"').nth(1))
        .expect("the server registers a custom method");
    assert!(
        ext.contains(&format!("'{method}'")) || ext.contains(&format!("\"{method}\"")),
        "the extension never asks for {method}"
    );
    assert!(
        ext.contains("setTextDocumentLanguage"),
        "the extension asks but does nothing with the answer"
    );
    // a contributed switch nothing reads is a promise the page keeps
    // and the code breaks
    assert!(
        v["contributes"]["configuration"]["properties"]
            .get("kamailioLsp.associateIncludedFiles")
            .is_some(),
        "the association must be switchable"
    );
    assert!(
        ext.contains("associateIncludedFiles"),
        "the setting is contributed but nothing reads it"
    );

    let events: Vec<&str> = v["activationEvents"]
        .as_array()
        .expect("activationEvents")
        .iter()
        .filter_map(|e| e.as_str())
        .collect();
    assert!(
        events.iter().any(|e| e.starts_with("workspaceContains:")),
        "nothing starts the extension when the only file open is an \
         unassociated .cfg, so the runtime association can never run: {events:?}"
    );
    // every statically claimed pattern is also an activation trigger:
    // a root matching it is what makes its fragments findable
    for p in v["contributes"]["languages"][0]["filenamePatterns"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let p = p.as_str().unwrap();
        assert!(
            events.contains(&format!("workspaceContains:**/{p}").as_str()),
            "a workspace holding {p} must start the extension: {events:?}"
        );
    }
}

#[test]
fn untrusted_workspaces_restrict_every_execution_vector() {
    // a workspace-committed settings.json in an UNTRUSTED folder must
    // not be able to point the extension at an attacker-controlled
    // binary: the server binary itself (serverPath), the checker
    // (kamailioPath), and the checker's dlopen path (modulesPath) all
    // have to be restricted
    let root = env!("CARGO_MANIFEST_DIR");
    let manifest = std::fs::read_to_string(format!("{root}/client/package.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let restricted: Vec<&str> =
        v["capabilities"]["untrustedWorkspaces"]["restrictedConfigurations"]
            .as_array()
            .expect("restrictedConfigurations present")
            .iter()
            .filter_map(|x| x.as_str())
            .collect();
    for key in [
        "kamailioLsp.kamailioPath",
        "kamailioLsp.serverPath",
        "kamailioLsp.modulesPath",
    ] {
        assert!(
            restricted.contains(&key),
            "{key} must be restricted in untrusted workspaces: {restricted:?}"
        );
    }
}
