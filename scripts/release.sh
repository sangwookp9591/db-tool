#!/usr/bin/env bash
set +x
set -euo pipefail
umask 077
cd "$(dirname "$0")/.."
version="${1:-}"
if [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || (( $# > 2 )) || [[ "${2:-}" != "" && "${2:-}" != --publish ]]; then
  echo 'Usage: scripts/release.sh <version> [--publish]' >&2
  exit 2
fi
# Refuse a mislabeled release before reading credentials or building.
python3 - "$version" <<'PY'
import json, pathlib, re, sys
root = pathlib.Path('.')
version = sys.argv[1]
cargo = re.search(r'(?m)^version\s*=\s*"([^"]+)"', (root/'Cargo.toml').read_text()).group(1)
assert cargo == json.loads((root/'src-tauri/tauri.conf.json').read_text())['version'] == json.loads((root/'ui/package.json').read_text())['version'] == version, 'Align Cargo.toml, tauri.conf.json and ui/package.json versions first'
assert (root/f'docs/release-notes/v{version}.md').read_text().strip(), 'Release notes are required'
assert not (root/f'dist/release/{version}').exists(), 'Release directory already exists; use a fresh version or move the previous artifacts for review'
PY
if ! command -v llvm-dlltool >/dev/null 2>&1; then
  export PATH="$(brew --prefix llvm)/bin:$PATH"
fi
command -v llvm-dlltool >/dev/null || { echo 'LLVM tools are required for Windows cross-compilation' >&2; exit 1; }
: "${APPLE_SIGNING_IDENTITY:?Set APPLE_SIGNING_IDENTITY to the Developer ID Application identity}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-u1}"
[[ "$CARGO_TARGET_DIR" = /* ]] || CARGO_TARGET_DIR="$PWD/$CARGO_TARGET_DIR"
key_path="$HOME/.tauri/axdb-updater.key"
[[ -f "$key_path" ]] || { echo 'Updater signing key is missing' >&2; exit 1; }
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s axdb-updater -a signing-key-password -w)"
# tauri build reads the key path from TAURI_SIGNING_PRIVATE_KEY; signer sign takes it explicitly with -f.
# The signer rejects KEY and KEY_PATH together; KEY accepts a path, so set only that.
export TAURI_SIGNING_PRIVATE_KEY="$key_path"
trap 'unset TAURI_SIGNING_PRIVATE_KEY_PASSWORD TAURI_SIGNING_PRIVATE_KEY' EXIT
out="$PWD/dist/release/$version"
mkdir -p "$out"
pnpm --dir ui install --frozen-lockfile
pnpm dlx @tauri-apps/cli@2.12.3 build --config src-tauri/tauri.conf.json --target universal-apple-darwin --bundles app,dmg
app="$CARGO_TARGET_DIR/universal-apple-darwin/release/bundle/macos/AxDB.app"
dmg="$CARGO_TARGET_DIR/universal-apple-darwin/release/bundle/dmg/AxDB_${version}_universal.dmg"
codesign --verify --deep --strict "$app"
# Staple the app before making the update archive; the early Tauri archive is pre-staple.
ditto -c -k --keepParent "$app" "$out/notarize-app.zip"
xcrun notarytool submit "$out/notarize-app.zip" --keychain-profile axdb-notary --wait --output-format json | python3 -c 'import json,sys; assert json.load(sys.stdin)["status"] == "Accepted", "App notarization failed"'
xcrun stapler staple "$app"
xcrun stapler validate "$app"
xcrun notarytool submit "$dmg" --keychain-profile axdb-notary --wait --output-format json | python3 -c 'import json,sys; assert json.load(sys.stdin)["status"] == "Accepted", "DMG notarization failed"'
xcrun stapler staple "$dmg"
xcrun stapler validate "$dmg"
archive="$out/AxDB_${version}_universal.app.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$archive" -C "$(dirname "$app")" AxDB.app
env -u TAURI_SIGNING_PRIVATE_KEY pnpm dlx @tauri-apps/cli@2.12.3 signer sign -f "$key_path" "$archive" >/dev/null
cp "$dmg" "$out/"
rm "$out/notarize-app.zip"
# Apple signing is macOS-only; Windows uses the independent updater signature.
env -u APPLE_SIGNING_IDENTITY pnpm dlx @tauri-apps/cli@2.12.3 build --config src-tauri/tauri.conf.json --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis
installer="$CARGO_TARGET_DIR/x86_64-pc-windows-msvc/release/bundle/nsis/AxDB_${version}_x64-setup.exe"
cp "$installer" "$installer.sig" "$out/"
python3 scripts/release-manifest.py "$version" "$out"
unset TAURI_SIGNING_PRIVATE_KEY_PASSWORD TAURI_SIGNING_PRIVATE_KEY
if [[ "${2:-}" == --publish ]]; then
  gh release create "v$version" --repo sangwookp9591/db-tool --latest --title "AxDB $version" --notes-file "docs/release-notes/v$version.md" "$out"/*.dmg "$out"/*.tar.gz "$out"/*.exe "$out"/*.sig "$out/latest.json"
else
  echo "Release artifacts ready: dist/release/$version (not published)"
fi
