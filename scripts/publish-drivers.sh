#!/usr/bin/env bash
set -euo pipefail
# Run only after release approval. This script does not generate or read private keys.
artifact_dir="${1:-dist/drivers}"
python3 - "$artifact_dir" <<'PY'
import hashlib, json, pathlib, subprocess, sys
root = pathlib.Path(sys.argv[1])
index = json.loads((root / 'index.json').read_text())
repo = 'sangwookp9591/db-tool'
assets = []
for driver in index['catalog']['drivers']:
    name = f"{driver['id']}-{driver['version']}.axdriver"
    path = root / name
    data = path.read_bytes()
    if len(data) != driver['size'] or hashlib.sha256(data).hexdigest() != driver['sha256']:
        raise SystemExit(f'Artifact integrity mismatch: {name}')
    tag = f"drivers-{driver['id']}-v{driver['version']}"
    expected = f'https://github.com/{repo}/releases/download/{tag}/{name}'
    if driver['url'] != expected:
        raise SystemExit('Unexpected package URL')
    assets.append((tag, path))
# Validate all files before the first remote mutation; publish the catalog last.
for tag, path in assets:
    exists = subprocess.run(['gh', 'release', 'view', tag, '--repo', repo], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
    if not exists:
        subprocess.run(['gh', 'release', 'create', tag, '--repo', repo, '--title', tag, '--notes', 'Signed AxDB driver package'], check=True)
    subprocess.run(['gh', 'release', 'upload', tag, str(path), '--repo', repo], check=True)
tag = 'drivers-index'
if subprocess.run(['gh', 'release', 'view', tag, '--repo', repo], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
    subprocess.run(['gh', 'release', 'create', tag, '--repo', repo, '--title', 'AxDB driver catalog', '--notes', 'Signed driver catalog'], check=True)
subprocess.run(['gh', 'release', 'upload', tag, str(root / 'index.json'), '--repo', repo, '--clobber'], check=True)
PY
