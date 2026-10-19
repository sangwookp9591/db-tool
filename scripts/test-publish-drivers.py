#!/usr/bin/env python3
"""Hermetic release-script checks: fake gh only; no GitHub or credentials."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("publish-drivers.sh").resolve()


class PublishTests(unittest.TestCase):
    def run_publish(self, corrupt=False):
        with tempfile.TemporaryDirectory(prefix="axdb-publish-test-") as directory:
            root = Path(directory)
            calls = root / "calls.jsonl"
            gh = root / "gh"
            gh.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['AXDB_TEST_GH_LOG'], 'a') as log: log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "sys.exit(1 if sys.argv[1:3] == ['release', 'view'] else 0)\n"
            )
            gh.chmod(0o755)
            drivers = []
            for driver_id in ["postgres", "sqlite"]:
                data = b"test artifact"
                (root / f"{driver_id}-1.0.0.axdriver").write_bytes(data)
                drivers.append({
                    "id": driver_id, "version": "1.0.0", "size": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "url": f"https://github.com/sangwookp9591/db-tool/releases/download/drivers-{driver_id}-v1.0.0/{driver_id}-1.0.0.axdriver",
                })
            if corrupt:
                drivers[-1]["sha256"] = "0" * 64
            (root / "index.json").write_text(json.dumps({"catalog": {"drivers": drivers}}))
            result = subprocess.run(
                ["bash", str(SCRIPT), str(root)], capture_output=True, text=True,
                env={**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"], "AXDB_TEST_GH_LOG": str(calls)},
            )
            return result, [json.loads(line) for line in calls.read_text().splitlines()] if calls.exists() else []

    def test_publish_packages_then_catalog(self):
        result, calls = self.run_publish()
        self.assertEqual(result.returncode, 0, result.stderr)
        uploads = [call for call in calls if call[:2] == ["release", "upload"]]
        self.assertEqual([call[2] for call in uploads], ["drivers-postgres-v1.0.0", "drivers-sqlite-v1.0.0", "drivers-index"])
        self.assertTrue(uploads[-1][3].endswith("index.json"))
        self.assertTrue(all("--clobber" not in call for call in uploads[:-1]))
        self.assertIn("--clobber", uploads[-1])

    def test_checks_every_digest_before_any_remote_call(self):
        result, calls = self.run_publish(corrupt=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("integrity mismatch", result.stderr)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
