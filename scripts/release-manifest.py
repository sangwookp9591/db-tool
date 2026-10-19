#!/usr/bin/env python3
"""Create an updater manifest from the final, signed release assets."""
import datetime
import json
import pathlib
import re
import sys


def manifest(version, directory, notes):
    if not re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)', version):
        raise ValueError('Expected a stable semantic version')
    root = pathlib.Path(directory)
    platforms = {}
    for platform, name in (
        ('darwin-aarch64', f'AxDB_{version}_universal.app.tar.gz'),
        ('darwin-x86_64', f'AxDB_{version}_universal.app.tar.gz'),
        ('windows-x86_64', f'AxDB_{version}_x64-setup.exe'),
    ):
        if not (root / name).stat().st_size:
            raise ValueError(f'Empty artifact: {name}')
        signature = (root / (name + '.sig')).read_text().strip()
        if not signature:
            raise ValueError(f'Missing signature: {name}')
        platforms[platform] = {
            'signature': signature,
            'url': f'https://github.com/sangwookp9591/db-tool/releases/download/v{version}/{name}',
        }
    # Strip in-app categories and date metadata for older updaters that render raw notes.
    text = re.sub(r'^(#{1,6} .*?)\s*\{(?:new|improved|fixed|good|bad|neutral)\}\s*$', r'\1', pathlib.Path(notes).read_text(), flags=re.M)
    text = re.sub(r'^<!--\s*date:\s*\d{4}-\d{2}-\d{2}\s*-->[ \t]*\r?\n?', '', text, flags=re.M)
    if not text.strip():
        raise ValueError('Release notes are empty')
    return {'version': version, 'notes': text, 'pub_date': datetime.datetime.now(datetime.timezone.utc).isoformat().replace('+00:00', 'Z'), 'platforms': platforms}


if __name__ == '__main__':
    version, directory = sys.argv[1:]
    result = manifest(version, directory, f'docs/release-notes/v{version}.md')
    path = pathlib.Path(directory) / 'latest.json'
    temporary = path.with_suffix('.json.tmp')
    temporary.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    temporary.replace(path)
