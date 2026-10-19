use anyhow::{bail, ensure, Context, Result};
use axdb_driver_package::{
    self as package, Catalog, CatalogEntry, Manifest, Platform, SignedCatalog, SigningKey,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};
use zeroize::Zeroizing;

const SERVICE: &str = "axdb-driver-signing";
const ACCOUNT: &str = "release-ed25519";
const IDS: &[(&str, &str, &str)] = &[
    ("postgres", "PostgreSQL", "Relational"),
    ("mysql", "MySQL / MariaDB", "Relational"),
    ("neo4j", "Neo4j", "Graph"),
    ("sqlite", "SQLite", "Relational"),
    ("mongodb", "MongoDB", "Document"),
    ("redis", "Redis", "KeyValue"),
];
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("gen-signing-key") => {
            ensure!(args.len() == 1, "gen-signing-key takes no arguments");
            ensure!(
                cfg!(target_os = "macos"),
                "Key generation requires macOS Keychain"
            );
            let entry = keyring::Entry::new(SERVICE, ACCOUNT)?;
            match entry.get_password() {
                Ok(secret) => {
                    let _secret = Zeroizing::new(secret);
                    bail!("Signing key already exists; refusing to replace it");
                }
                Err(keyring::Error::NoEntry) => {}
                Err(error) => return Err(error.into()),
            }
            let mut seed = Zeroizing::new([0u8; 32]);
            getrandom::fill(seed.as_mut())
                .map_err(|error| anyhow::anyhow!("OS randomness unavailable: {error}"))?;
            let key = SigningKey::from_bytes(&seed);
            entry.set_password(&Zeroizing::new(hex::encode(seed.as_ref())))?;
            println!(
                "AXDB_OFFICIAL_DRIVER_PUBLIC_KEY={}",
                hex::encode(key.verifying_key().to_bytes())
            );
            Ok(())
        }
        Some("package-drivers") => package_drivers(&args[1..]),
        Some("--help") | None => {
            println!("cargo xtask gen-signing-key\ncargo xtask package-drivers --target <triple> [--target <triple> ...] [--driver <id>] [--version <semver>] [--out <directory>] [--notarize] [--linux-docker]");
            Ok(())
        }
        _ => bail!("Unknown xtask; use --help"),
    }
}
fn signing_key() -> Result<SigningKey> {
    let secret = Zeroizing::new(match std::env::var("AXDB_DRIVER_SIGNING_KEY") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => keyring::Entry::new(SERVICE, ACCOUNT)?
            .get_password()
            .context(
                "Set AXDB_DRIVER_SIGNING_KEY (hex seed), or run cargo xtask gen-signing-key",
            )?,
        Err(error) => return Err(error.into()),
    });
    let bytes =
        Zeroizing::new(hex::decode(secret.trim()).context("Signing key must be hexadecimal")?);
    let seed: &[u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Signing key must contain 32 bytes"))?;
    Ok(SigningKey::from_bytes(seed))
}
fn run(command: &mut Command) -> Result<()> {
    // Build/sign subprocesses never inherit the package signing secret.
    let status = command.env_remove("AXDB_DRIVER_SIGNING_KEY").status()?;
    ensure!(status.success(), "Command failed: {command:?}");
    Ok(())
}
fn package_drivers(args: &[String]) -> Result<()> {
    let mut targets = Vec::new();
    let mut ids = Vec::new();
    let mut version = "0.1.0".to_string();
    let mut out = PathBuf::from("dist/drivers");
    let mut notarize = false;
    let mut linux_docker = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--target" | "--driver" | "--version" | "--out" => {
                let flag = &args[i];
                i += 1;
                let value = args.get(i).context("Missing option value")?;
                match flag.as_str() {
                    "--target" => {
                        ensure!(
                            package::TARGETS.contains(&value.as_str()),
                            "Unsupported target"
                        );
                        targets.push(value.clone());
                    }
                    "--driver" => {
                        ensure!(IDS.iter().any(|entry| entry.0 == value), "Unknown driver");
                        ids.push(value.clone());
                    }
                    "--version" => version = value.clone(),
                    "--out" => out = value.into(),
                    _ => unreachable!(),
                }
            }
            "--notarize" => notarize = true,
            "--linux-docker" => linux_docker = true,
            _ => bail!("Unknown option: {}", args[i]),
        }
        i += 1;
    }
    ensure!(!targets.is_empty(), "Specify at least one --target");
    if ids.is_empty() {
        ids = IDS.iter().map(|entry| entry.0.to_string()).collect();
    }
    targets.sort();
    targets.dedup();
    ids.sort();
    ids.dedup();
    let key = signing_key()?;
    let target_dir =
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap_or_else(|| "target-p2".into()));
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("Workspace root unavailable")?;
    std::env::set_current_dir(root)?;
    let mut catalog = Catalog::default();
    for id in ids {
        let (_, name, paradigm) = IDS
            .iter()
            .find(|entry| entry.0 == id)
            .context("Unknown driver")?;
        let mut manifest = Manifest {
            id: id.clone(),
            icon_key: Some(id.clone()),
            name: name.to_string(),
            version: version.clone(),
            protocol: 1,
            description: format!("{name} driver for AxDB"),
            paradigm: paradigm.to_string(),
            homepage: format!("https://github.com/{}", package::REPOSITORY),
            license: "MIT".into(),
            platforms: BTreeMap::new(),
        };
        let mut binaries = BTreeMap::new();
        for target in &targets {
            let executable = format!(
                "axdb-driver-{id}{}",
                if target.contains("windows") {
                    ".exe"
                } else {
                    ""
                }
            );
            if target.contains("linux") && linux_docker {
                let target_relative = target_dir.strip_prefix(root).unwrap_or(&target_dir);
                ensure!(
                    !target_relative.is_absolute()
                        && target_relative
                            .components()
                            .all(|part| matches!(part, std::path::Component::Normal(_))),
                    "Docker requires CARGO_TARGET_DIR inside workspace"
                );
                let platform = if target.starts_with("aarch64") {
                    "linux/arm64"
                } else {
                    "linux/amd64"
                };
                run(Command::new("docker")
                    .args(["run", "--rm", "--platform", platform, "-v"])
                    .arg(format!("{}:/work", root.display()))
                    .args(["-w", "/work", "-e"])
                    .arg(format!(
                        "CARGO_TARGET_DIR=/work/{}",
                        target_relative.display()
                    ))
                    .args([
                        "rust:1.94",
                        "cargo",
                        "build",
                        "--release",
                        "--target",
                        target,
                        "--bin",
                        &format!("axdb-driver-{id}"),
                    ]))?;
            } else {
                ensure!(
                    !target.contains("linux") || cfg!(target_os = "linux"),
                    "Use --linux-docker to build Linux drivers on this host"
                );
                let mut build = Command::new("cargo");
                if target.contains("windows") && !cfg!(target_os = "windows") {
                    build.arg("xwin");
                }
                build
                    .args([
                        "build",
                        "--release",
                        "--target",
                        target,
                        "--bin",
                        &format!("axdb-driver-{id}"),
                    ])
                    .env("CARGO_TARGET_DIR", &target_dir);
                run(&mut build)?;
            }
            let binary = target_dir.join(target).join("release").join(&executable);
            if target.contains("apple-darwin") {
                if let Ok(identity) = std::env::var("APPLE_SIGNING_IDENTITY") {
                    run(Command::new("codesign")
                        .args([
                            "--force",
                            "--options",
                            "runtime",
                            "--timestamp",
                            "--sign",
                            &identity,
                        ])
                        .arg(&binary))?;
                } else {
                    ensure!(!notarize, "--notarize requires APPLE_SIGNING_IDENTITY");
                }
                if notarize {
                    let archive = binary.with_extension("notary.zip");
                    run(Command::new("ditto")
                        .args(["-c", "-k", "--keepParent"])
                        .arg(&binary)
                        .arg(&archive))?;
                    let result = run(Command::new("xcrun")
                        .args(["notarytool", "submit"])
                        .arg(&archive)
                        .args(["--keychain-profile", "axdb-notary", "--wait"]));
                    let _ = std::fs::remove_file(archive);
                    result?;
                }
            }
            let data = std::fs::read(&binary)
                .with_context(|| format!("Cannot read {}", binary.display()))?;
            manifest.platforms.insert(
                target.clone(),
                Platform {
                    path: format!("bin/{target}/{executable}"),
                    sha256: package::sha256(&data),
                },
            );
            binaries.insert(target.clone(), data);
        }
        let bytes = package::write_package(&manifest, &binaries, Some(&key))?;
        // Verify our output before publishing any artifact.
        let trusted = [package::TrustedKey {
            public_key: key.verifying_key().to_bytes(),
        }];
        package::read_package(&bytes, &trusted, false)?;
        std::fs::create_dir_all(&out)?;
        std::fs::write(out.join(format!("{id}-{version}.axdriver")), &bytes)?;
        catalog.drivers.push(CatalogEntry {
            id: id.clone(),
            icon_key: manifest.icon_key.clone(),
            name: name.to_string(),
            version: version.clone(),
            description: manifest.description,
            paradigm: manifest.paradigm,
            homepage: manifest.homepage,
            license: manifest.license,
            platforms: targets.clone(),
            url: package::package_url(&id, &version),
            size: bytes.len() as u64,
            sha256: package::sha256(&bytes),
        });
    }
    let index = SignedCatalog::new(catalog, &key)?;
    std::fs::write(out.join("index.json"), serde_json::to_vec_pretty(&index)?)?;
    println!(
        "Packaged drivers in {}; signing public key: {}",
        out.display(),
        hex::encode(key.verifying_key().to_bytes())
    );
    Ok(())
}
