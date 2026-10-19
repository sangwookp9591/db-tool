fn main() {
    // package-drivers emits the signed index; absent development snapshots stay empty.
    let snapshot = std::path::Path::new("../dist/drivers/index.json");
    println!("cargo:rerun-if-changed={}", snapshot.display());
    let bytes = match std::fs::read(snapshot) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => panic!("Cannot read driver catalog snapshot: {error}"),
    };
    assert!(
        bytes.len() <= 2 * 1024 * 1024,
        "Driver catalog snapshot exceeds 2 MiB"
    );
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("driver-index.json"), bytes).unwrap();
    tauri_build::build()
}
