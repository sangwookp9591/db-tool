set -e
apt-get update -qq >/dev/null && apt-get install -y -qq gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 >/dev/null
rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1
cd /src && export CARGO_TARGET_DIR=/target-win
cargo build --release --locked --target x86_64-pc-windows-gnu -p axdb 2>&1 | grep -vE "^ *(Compiling|Downloaded|Downloading)" | tail -25
ls -la /target-win/x86_64-pc-windows-gnu/release/axdb.exe && file /target-win/x86_64-pc-windows-gnu/release/axdb.exe 2>/dev/null || true
