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
    assert_eq!(lang["firstLine"], "^#!KAMAILIO");
}
