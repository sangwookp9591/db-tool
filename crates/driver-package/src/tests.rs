use super::*;
fn fixture() -> (
    Manifest,
    BTreeMap<String, Vec<u8>>,
    SigningKey,
    Vec<TrustedKey>,
) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let binary = b"executable".to_vec();
    let target = TARGETS[0].to_string();
    (
        Manifest {
            id: "sqlite".into(),
            icon_key: None,
            name: "SQLite".into(),
            version: "1.0.0".into(),
            protocol: 1,
            description: String::new(),
            paradigm: "Relational".into(),
            homepage: String::new(),
            license: "MIT".into(),
            platforms: [(
                target.clone(),
                Platform {
                    path: format!("bin/{target}/driver"),
                    sha256: sha256(&binary),
                },
            )]
            .into(),
        },
        [(target, binary)].into(),
        key.clone(),
        vec![TrustedKey {
            public_key: key.verifying_key().to_bytes(),
        }],
    )
}
#[test]
fn strict_signatures_and_consent() {
    let (manifest, binaries, key, trusted) = fixture();
    let signed = write_package(&manifest, &binaries, Some(&key)).unwrap();
    assert!(read_package(&signed, &trusted, false).unwrap().trusted);
    assert!(read_package(&signed, &[], false).is_err());
    assert!(!read_package(&signed, &[], true).unwrap().trusted);
    let unsigned = write_package(&manifest, &binaries, None).unwrap();
    assert!(read_package(&unsigned, &trusted, false).is_err());
    assert!(read_package(&unsigned, &trusted, true).is_ok());
    let signature = sign(b"manifest", &key);
    assert!(verify(b"tampered", &signature, &trusted).is_err());
    assert!(!verify(
        b"manifest",
        &sign(b"manifest", &SigningKey::from_bytes(&[8; 32])),
        &trusted
    )
    .unwrap());
    let mut bad = signature;
    bad.signature = "00".repeat(64);
    assert!(verify(b"manifest", &bad, &trusted).is_err());
    assert!(verify_sha256(b"tampered", &sha256(b"original")).is_err());
}
#[test]
fn zip_slip_and_links_rejected() {
    for path in [
        "../escape",
        "/tmp/escape",
        "bin/../../escape",
        "C:/escape",
        "bin\\escape",
        "./escape",
    ] {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(path, SimpleFileOptions::default()).unwrap();
        zip.write_all(b"bad").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(read_package(&bytes, &[], true).is_err(), "{path}");
    }
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_symlink("manifest.json", "/tmp/escape", SimpleFileOptions::default())
        .unwrap();
    assert!(read_package(&zip.finish().unwrap().into_inner(), &[], true).is_err());
}
#[test]
fn atomic_stage_rolls_back_and_preserves_previous() {
    let (manifest, binaries, key, trusted) = fixture();
    let verified = read_package(
        &write_package(&manifest, &binaries, Some(&key)).unwrap(),
        &trusted,
        false,
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let previous = root.path().join("sqlite/0.9.0");
    std::fs::create_dir_all(&previous).unwrap();
    std::fs::write(previous.join("keep"), b"previous").unwrap();
    {
        let staged = verified.stage(root.path(), TARGETS[0]).unwrap();
        assert!(staged.executable.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&staged.executable)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
    }
    assert!(!root.path().join("sqlite/1.0.0").exists());
    assert!(previous.join("keep").exists());
    verified.stage(root.path(), TARGETS[0]).unwrap().commit();
    assert!(root.path().join("sqlite/1.0.0/manifest.json").exists());
    assert!(verified.stage(root.path(), TARGETS[0]).is_err());
}
#[test]
fn catalog_signature_cannot_be_bypassed_or_tampered() {
    let (_, _, key, trusted) = fixture();
    let signed = SignedCatalog::new(Catalog::default(), &key).unwrap();
    assert!(signed.clone().verified(&trusted).is_ok());
    assert!(signed.verified(&[]).is_err());
}

#[test]
fn signed_manifest_rejects_tampered_executable() {
    let (manifest, _, key, trusted) = fixture();
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    let data = serde_json::to_vec(&manifest).unwrap();
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(&data).unwrap();
    zip.start_file("manifest.sig", options).unwrap();
    zip.write_all(&serde_json::to_vec(&sign(&data, &key)).unwrap())
        .unwrap();
    zip.start_file(&manifest.platforms[TARGETS[0]].path, options)
        .unwrap();
    zip.write_all(b"modified executable").unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert!(read_package(&bytes, &trusted, false)
        .err()
        .unwrap()
        .to_string()
        .contains("SHA-256 mismatch"));
}

#[test]
fn cancelled_validation_and_extraction_leave_no_candidate() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (manifest, binaries, key, trusted) = fixture();
    let bytes = write_package(&manifest, &binaries, Some(&key)).unwrap();
    assert!(read_package_cancellable(&bytes, &trusted, false, || true)
        .err()
        .unwrap()
        .to_string()
        .contains("cancelled"));
    let checks = AtomicUsize::new(0);
    assert!(read_package_cancellable(&bytes, &trusted, false, || checks
        .fetch_add(1, Ordering::Relaxed)
        >= 2)
    .err()
    .unwrap()
    .to_string()
    .contains("cancelled"));
    let package = read_package(&bytes, &trusted, false).unwrap();
    let root = tempfile::tempdir().unwrap();
    let checks = AtomicUsize::new(0);
    assert!(package
        .stage_cancellable(root.path(), TARGETS[0], || checks
            .fetch_add(1, Ordering::Relaxed)
            >= 2)
        .is_err());
    assert!(!root.path().join("sqlite/1.0.0").exists());
    assert_eq!(
        std::fs::read_dir(root.path().join("sqlite"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn optional_icon_key_preserves_legacy_signatures_and_is_signed_when_present() {
    let (mut manifest, binaries, key, trusted) = fixture();
    let legacy = serde_json::to_vec(&manifest).unwrap();
    assert!(!String::from_utf8_lossy(&legacy).contains("icon_key"));
    let decoded: Manifest = serde_json::from_slice(&legacy).unwrap();
    assert!(decoded.icon_key.is_none());
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), legacy);
    manifest.icon_key = Some("sqlite".into());
    let bytes = write_package(&manifest, &binaries, Some(&key)).unwrap();
    assert_eq!(
        read_package(&bytes, &trusted, false)
            .unwrap()
            .manifest
            .icon_key
            .as_deref(),
        Some("sqlite")
    );
    let signature = sign(&serde_json::to_vec(&manifest).unwrap(), &key);
    manifest.icon_key = Some("redis".into());
    assert!(verify(
        &serde_json::to_vec(&manifest).unwrap(),
        &signature,
        &trusted
    )
    .is_err());
    let entry = CatalogEntry {
        id: "sqlite".into(),
        icon_key: None,
        name: "SQLite".into(),
        version: "1.0.0".into(),
        description: String::new(),
        paradigm: "Relational".into(),
        homepage: String::new(),
        license: "MIT".into(),
        platforms: vec![TARGETS[0].into()],
        url: package_url("sqlite", "1.0.0"),
        size: 1,
        sha256: sha256(b"x"),
    };
    let signed = SignedCatalog::new(
        Catalog {
            drivers: vec![entry],
        },
        &key,
    )
    .unwrap();
    let legacy = serde_json::to_vec(&signed).unwrap();
    assert!(!String::from_utf8_lossy(&legacy).contains("icon_key"));
    let decoded: SignedCatalog = serde_json::from_slice(&legacy).unwrap();
    assert!(decoded.verified(&trusted).is_ok());
    let mut tampered = signed.clone();
    tampered.catalog.drivers[0].icon_key = Some("sqlite".into());
    assert!(tampered.verified(&trusted).is_err());
    let mut catalog = signed.catalog;
    catalog.drivers[0].icon_key = Some("sqlite".into());
    assert_eq!(
        SignedCatalog::new(catalog, &key)
            .unwrap()
            .verified(&trusted)
            .unwrap()
            .drivers[0]
            .icon_key
            .as_deref(),
        Some("sqlite")
    );
}
