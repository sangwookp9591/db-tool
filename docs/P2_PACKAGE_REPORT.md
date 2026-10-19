# P2: installable driver packages

Contract: [ADR-0003](adr/0003-installable-drivers.md). Implementation and scoped verification are complete. Work is local and uncommitted; the running `/Applications/AxDB.app`, its GUI, and user databases were not used.

## Delivered

`crates/driver-package` implements `.axdriver` ZIP creation and verification, the manifest/platform schema, SHA-256, strict Ed25519 signatures, a signed single-file catalog, safe staging and rollback. `src-tauri/src/driver_install.rs` implements cached catalog reads with background ETag refresh and the install/update/remove commands. `xtask` builds selected P1 executable targets, signs and packages them, and writes `index.json`; `scripts/publish-drivers.sh` uploads verified local artifacts through `gh release` only when explicitly run.

Installation is download → package digest → manifest signature/executable digests → temporary directory → atomic directory rename → real P1 `PluginDriver::load_cancelable(PathBuf, CancelToken)` handshake → active marker → Registry update. Handshake protocol validation is provided by P1; P2 additionally compares the id, display name and paradigm with the signed manifest. Until activation, the old marker and Registry entry remain usable. Cancellation interrupts requests, checks ZIP decompression/writes every 64 KiB, awaits extraction cleanup and kills/reaps the handshake probe before deleting the candidate. Cancel and activation share one lock, so a committed operation cannot be cancelled. The former active version is retained and older versions are pruned; a failed cleanup is logged without falsely reporting that a successful activation failed. Removal rejects drivers with open host sessions.

Only HTTPS is allowed in production, including redirects, with at most three redirects and a 90-second request timeout. Downloaded packages are capped at 256 MiB, expanded contents at 512 MiB and the index at 2 MiB. Traversal, absolute/Windows escape paths, duplicate/unexpected entries, symlinks and special ZIP files are rejected. Executables receive mode 0755 on Unix. Artifact reads reject named pipes and non-regular files before opening. Explicit consent permits an unsigned or unknown-key local file; malformed signatures and tampered executable hashes always fail. Remote catalog and package installs require known signing keys.

## Shared interfaces

P1 dispatch `ctx_4a9cf7d18b79` and P2 agreed on `PluginDriver::load(PathBuf)` and subsequently `load_cancelable(PathBuf, CancelToken)` with awaited probe termination, interior-mutability Registry `register(&self, Arc<dyn Driver>)`/`unregister(&self, id)`, and `installed_executables(root)` where root is the drivers directory. P1 owns the Registry change, local startup loader, and `Host::new` initialization. P2 owns the marker, package validation and installation logic. P3 dispatch `ctx_80691b3d1aea` accepted these host command contracts:

- `drivers_catalog()` returns `{drivers, installed:[{id,version}], refreshing, warning?}` immediately from local state, then emits `drivers-catalog-updated` after one conditional index request. Catalog entries include `installed_version` and the signed metadata. Neither `Installer::default` nor startup discovery initiates a request. Disk bytes are re-read synchronously; signature verification is memoized by their SHA-256 and invalidated on every byte change, preserving integrity without repeating cryptographic work on each UI read.
- `driver_install({source, allowUnverified?, onProgress})` accepts a catalog id or absolute `.axdriver` path and returns `{id,version}`. `onProgress` is a Tauri Channel with `{stage,downloaded,total}`; stages are download, verify, signature, extract, handshake, register, complete. Progress also includes `bytes` and `bytesPerSecond`; a bounded watch channel coalesces intermediate events and sends at most once per 100 ms, including the final event.
- `driver_update({id,onProgress})`, `driver_remove({id})`, and `driver_pick_file()` complete the lifecycle. `driver_cancel()` returns true when an active operation is cancelled and false once committing has begun or no operation is active.

Installed layout:

```text
<app-data>/drivers/
  catalog-cache.json             # signed index plus ETag
  <id>/
    current.json                 # {version,executable,allow_unverified}
    <version>/manifest.json
    <version>/manifest.sig       # omitted only for consented unsigned local packages
    <version>/bin/<target>/<exe>
    <previous-version>/...
```

Startup reads active markers only, verifies manifest signatures/consent and executable digests locally, then P1 probes the executables. Debug `AXDB_DRIVER_DIR` is P1's separate development path.

## Signing and packaging

```sh
# Generates a fresh Ed25519 seed directly into macOS Keychain, refusing overwrite.
# Prints only the public key. Never writes a private-key file.
CARGO_TARGET_DIR=target-p2 cargo xtask gen-signing-key

# Embed the printed public key when building the app; optionally retain the old key.
AXDB_OFFICIAL_DRIVER_PUBLIC_KEY='<64 hex public-key characters>' \
AXDB_PREVIOUS_DRIVER_PUBLIC_KEY='<optional previous public key>' \
CARGO_TARGET_DIR=target-p2 cargo build -p axdb --release

# Reads AXDB_DRIVER_SIGNING_KEY (32-byte hex seed) or macOS Keychain:
# service=axdb-driver-signing, account=release-ed25519.
CARGO_TARGET_DIR=target-p2 cargo xtask package-drivers \
  --target aarch64-apple-darwin --target x86_64-apple-darwin \
  --target x86_64-pc-windows-msvc --out dist/drivers

# Optional Linux build via Docker (Rust 1.94 image, matching container architecture).
CARGO_TARGET_DIR=target-p2 cargo xtask package-drivers \
  --target x86_64-unknown-linux-gnu --linux-docker

# Only after coordinator release approval:
scripts/publish-drivers.sh dist/drivers
```

The current and previous public-key constants form the official trust list. A build with no provisioned public key fails closed for remote catalog/install operations; it still permits explicitly consented local packages. No arbitrary production signing identity was generated during this task. Invalid/absent public keys never become trusted implicitly. Rotate by embedding the new current key and retaining the old key as previous, then release the app before switching catalog signing.

`--driver <id>` narrows package output; default builds all five official drivers. Each invocation emits a catalog for the selected drivers, so production publication must use the complete intended catalog. Windows uses `cargo xwin build` on macOS. macOS binaries use `codesign --options runtime --timestamp` if `APPLE_SIGNING_IDENTITY` is provided. `--notarize` additionally submits a temporary ZIP to `xcrun notarytool` using Keychain profile `axdb-notary`; the temporary ZIP is removed afterward. Build/sign subprocesses do not inherit `AXDB_DRIVER_SIGNING_KEY`, and secret buffers are zeroized.

Registry URLs are constants: catalog at release `drivers-index/index.json`, packages at release `drivers-<id>-v<version>/<id>-<version>.axdriver`, repository `sangwookp9591/db-tool`. The publish script checks all package sizes, digests and URLs before its first remote mutation and publishes the index last. Versioned package assets are never overwritten; only the catalog uses `--clobber`.

## Verification

Executed checks (all builds use `CARGO_TARGET_DIR=target-p2`):

| Check | Result |
|---|---|
| `cargo test -p axdb-driver-package -p xtask` | 6 package tests passed; xtask output is also self-verified and exercised by the native packaging command |
| `cargo test -p axdb --lib driver_install::tests --no-default-features -- --skip http_catalog_install_handshake_rollback_update_remove --nocapture` | 6 tests passed: transport/markers, regular-file checks, header/stream limits, consent/failed handshake, 10 Hz coalescing/final delivery, cache integrity/latency |
| `AXDB_PACKAGE_TEST_BINARY=<built SDK example-kv> cargo test -p axdb --lib http_catalog_install_handshake_rollback_update_remove --no-default-features -- --ignored --nocapture` | 1 passed: real HTTP/ETag, signatures, install/handshake/Registry, failed upgrade rollback, retained version count, deletion, consent, startup hash checks and cancellation in every pre-commit stage |
| `cargo test -p axdb --lib cancelling_handshake_reaps_probe_before_returning --no-default-features` | 1 P1 probe test passed: real child terminated/reaped before return |
| `python3 scripts/test-publish-drivers.py` | 2 passed; fake gh only, including all-file preflight and immutable package assets/catalog-last ordering |
| `cargo clippy -p axdb-driver-package -p xtask -p axdb --all-targets --no-default-features -- -D warnings` | Passed |
| Scoped `rustfmt --check`, `git diff --check`, `bash -n`, report local links | Passed |

Native smoke: `cargo xtask package-drivers --driver sqlite --target aarch64-apple-darwin --out target-p2/package-smoke` rebuilt the P1 release executable, signed the manifest and catalog with an environment-provided test key, and verified the generated package. Final artifact: `sqlite-0.1.0.axdriver`, **1,788,969 bytes**, SHA-256 `8b35161b1617df5e8560e844feaae445f45e64de8e1893bdcfdbdbdf9d176d20`. Independent Python ZIP inspection confirmed archive layout and all executable digests. No real signing key was written or generated.

A supplemental release-mode example-kv fixture build was stopped when it selected a different feature graph and began redundant dependency compilation. The integration tests use the actual built SDK debug executable with symbols stripped; this does not change its handshake behavior.

### Measured latency

The current five-entry signed disk-catalog test, run by itself: **cold 3.324 ms**, 100 warm reads **p50 0.228 ms / p95 0.593 ms**. The cache-path target of under 5 ms was met in this isolated backend measurement. Disk bytes are still checked on every read; changing a signed field invalidates the memoized verification and is rejected. The separately recorded run with six concurrent tests and Cargo builds measured cold 6.154 ms, warm p50 0.337 ms / p95 7.005 ms; that loaded sample exceeded the target. An earlier one-entry sample before memoization had p50 1.034 ms / p95 12.195 ms; these are different fixtures/load conditions, not an apples-to-apples benchmark.

The latest full HTTP install core took **1,291.698 ms** for the real stripped SDK fixture. Cancellation-to-cleanup durations were verify **0.272 ms**, signature **0.073 ms**, extraction **0.104 ms**, handshake **0.910 ms**, and pre-registration **3.986 ms**; download cancellation was also verified before registration. Committed operations return false from `driver_cancel`. These are local debug-host/backend timings with loopback HTTP; they exclude frontend IPC, TLS/network latency and the progress channel's final coalescing interval (up to 100 ms). They do not establish production/device performance or a hard deadline under system load.

The local integration test has a test-only loopback HTTP client, real ephemeral TCP listener and real P1 SDK executable. Production has no HTTP override or runtime custom-trust-key environment switch. It opens no DB connections.

The cryptographic implementation uses [ed25519-dalek strict verification](https://docs.rs/ed25519-dalek/2.2.0/ed25519_dalek/struct.VerifyingKey.html#method.verify_strict); ZIP inspection uses [zip archive entry metadata](https://docs.rs/zip/9.0.5/zip/read/struct.ZipFile.html) with explicit path and file-type validation before filesystem extraction. ZIP is pinned to the patched 9.0 release line following [RUSTSEC-2026-0369](https://rustsec.org/advisories/RUSTSEC-2026-0369.html); automatic ZIP extraction APIs are not used.

## Scope and remaining release gates

Initial shared dirty files included Cargo.lock, src-tauri/Cargo.toml, src-tauri/src/lib.rs, agent_bridge.rs, several UI files and E1/O1 untracked files. Those changes were preserved. P2 changes only its new package/xtask/installer/test/report/publish/config files plus workspace dependency entries and minimal module/state/command registrations; concurrent P1/E1/O1/P3 edits remain owned by those lanes.

Production key provisioning, Developer ID signing/notarization, cross-platform runtime verification and GitHub release upload require release-owner credentials and were not performed. No commit or push was made.
