use super::*;
use std::io::{Cursor, Read};

#[path = "../tests/fixtures/v0_2_0.rs"]
mod legacy;

#[test]
fn generated_artifacts_are_installable_by_v0_2_0() {
    let out = tempfile::tempdir().unwrap();
    let key = SigningKey::from_bytes(&[42; 32]);
    let trusted = [package::TrustedKey {
        public_key: key.verifying_key().to_bytes(),
    }];
    let mut catalog = Catalog::default();
    for &(id, name, paradigm) in IDS {
        let mut manifest = Manifest {
            id: id.into(),
            icon_key: Some(id.into()),
            name: name.into(),
            version: "0.3.0".into(),
            protocol: 1,
            description: format!("{name} driver for AxDB"),
            paradigm: paradigm.into(),
            homepage: format!("https://github.com/{}", package::REPOSITORY),
            license: "MIT".into(),
            platforms: BTreeMap::new(),
        };
        let mut binaries = BTreeMap::new();
        for &target in package::TARGETS {
            let binary = format!("test executable: {id} {target}").into_bytes();
            manifest.platforms.insert(
                target.into(),
                Platform {
                    path: format!("bin/{target}/axdb-driver-{id}"),
                    sha256: package::sha256(&binary),
                },
            );
            binaries.insert(target.into(), binary);
        }
        catalog
            .drivers
            .push(write_driver_package(out.path(), manifest, &binaries, &key).unwrap());
    }
    write_catalogs(out.path(), catalog, &key).unwrap();
    let bytes = std::fs::read(out.path().join("index.json")).unwrap();
    let v1: legacy::SignedCatalog = serde_json::from_slice(&bytes).unwrap();
    let signature = package::DetachedSignature {
        public_key: v1.signature.public_key,
        signature: v1.signature.signature,
    };
    assert!(package::verify(
        &serde_json::to_vec(&v1.catalog).unwrap(),
        &signature,
        &trusted
    )
    .unwrap());
    assert_eq!(v1.catalog.drivers.len(), IDS.len());
    for entry in v1.catalog.drivers {
        let bytes = std::fs::read(
            out.path()
                .join(format!("{}-{}.axdriver", entry.id, entry.version)),
        )
        .unwrap();
        assert_eq!(bytes.len() as u64, entry.size);
        package::verify_sha256(&bytes, &entry.sha256).unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut manifest_bytes = Vec::new();
        zip.by_name("manifest.json")
            .unwrap()
            .read_to_end(&mut manifest_bytes)
            .unwrap();
        let manifest: legacy::Manifest = serde_json::from_slice(&manifest_bytes).unwrap();
        assert_eq!(manifest.id, entry.id);
        assert_eq!(serde_json::to_vec(&manifest).unwrap(), manifest_bytes);
        let signature: package::DetachedSignature =
            serde_json::from_reader(zip.by_name("manifest.sig").unwrap()).unwrap();
        assert!(package::verify(&manifest_bytes, &signature, &trusted).unwrap());
        for platform in manifest.platforms.values() {
            let mut binary = Vec::new();
            zip.by_name(&platform.path)
                .unwrap()
                .read_to_end(&mut binary)
                .unwrap();
            package::verify_sha256(&binary, &platform.sha256).unwrap();
        }
    }
    let bytes = std::fs::read(out.path().join("index-v2.json")).unwrap();
    assert!(serde_json::from_slice::<legacy::SignedCatalog>(&bytes).is_err());
    let signed: SignedCatalog = serde_json::from_slice(&bytes).unwrap();
    let v2 = signed.clone().verified(&trusted).unwrap();
    for entry in v2.drivers {
        assert_eq!(entry.icon_key.as_deref(), Some(entry.id.as_str()));
    }
    let mut tampered = signed;
    tampered.catalog.drivers[0].icon_key = Some("tampered".into());
    assert!(tampered.verified(&trusted).is_err());
    let mut unknown: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    unknown["catalog"]["drivers"][0]["future_field"] = true.into();
    assert!(serde_json::from_value::<SignedCatalog>(unknown).is_err());
}
