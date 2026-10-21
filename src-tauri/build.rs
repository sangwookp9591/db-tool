// Build scripts talk to Cargo through stdout (`cargo:` directives); that is protocol output, not diagnostics.
#[allow(clippy::print_stdout)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // package-drivers emits the signed index; absent development snapshots stay empty.
    let snapshot = std::path::Path::new("../dist/drivers/index-v2.json");
    println!("cargo:rerun-if-changed={}", snapshot.display());
    let bytes = match std::fs::read(snapshot) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(format!("Cannot read driver catalog snapshot: {error}").into()),
    };
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Driver catalog snapshot exceeds 2 MiB".into());
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    std::fs::write(out.join("driver-index.json"), bytes)?;
    tauri_build::build();
    Ok(())
}
