#!/usr/bin/env bash
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
strict="${POLICY_STRICT:-0}"
failed=0
run() {
  local label="$1"; shift
  printf '\n[%s] %s\n' "$label" "$*"
  if "$@"; then printf 'PASS: %s\n' "$label"; else printf 'WARN: %s failed (strict=%s)\n' "$label" "$strict"; failed=1; fi
}
run 'Rust format' cargo fmt --all --check
if [[ "$strict" == 1 ]]; then
  run 'Rust Clippy' cargo clippy --workspace --all-targets -- -D warnings
else
  run 'Rust Clippy report' cargo clippy --workspace --all-targets
fi
if command -v cargo-deny >/dev/null 2>&1; then run 'cargo-deny' cargo deny check; else printf '\nSKIP: cargo-deny is not installed\n'; fi
if [[ -f ui/package.json ]]; then run 'UI typecheck' bash -c 'cd ui && npx tsc --noEmit -p tsconfig.json'; fi
printf '\nPolicy check summary: strict=%s, failures=%s\n' "$strict" "$failed"
if [[ "$strict" == 1 && "$failed" -ne 0 ]]; then exit 1; fi
exit 0
