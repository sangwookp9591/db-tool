"""Run W1 tests against only the explicitly isolated test fixtures."""
import os
from pathlib import Path
import subprocess
import time
import tempfile

root = Path(Path("/tmp/axdb-w1-network-fixture-path").read_text().strip())
if not root.name.startswith("axdb-w1-network-") or root.parent.resolve() != Path(tempfile.gettempdir()).resolve():
    raise SystemExit("Unexpected network fixture directory")
socket = root / "agent.sock"
if socket.exists():
    socket.unlink()
agent = subprocess.Popen(["ssh-agent", "-D", "-a", str(socket)],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
env = dict(os.environ,
           CARGO_TARGET_DIR="target-w1", CARGO_BUILD_JOBS="2", CARGO_INCREMENTAL="0",
           AXDB_DRIVER_DIR=str(Path("target-w1/debug").resolve()),
           AXDB_TEST_NETWORK_FIXTURE=str(root / "fixture.json"),
           AXDB_TEST_PG="host=127.0.0.1 port=55432 user=postgres password=testpw dbname=axdb_test",
           AXDB_TEST_MONGO="mongodb://axdb:testmongo@127.0.0.1:57017/?authSource=admin",
           AXDB_TEST_NEO4J="bolt://127.0.0.1:57687|neo4j|testpassword",
           AXDB_TEST_REDIS="redis://:testredis@127.0.0.1:56379/0",
           AXDB_DEV_SECRET_STORE=str(root / "secrets.json"),
           SSH_AUTH_SOCK=str(socket))
try:
    for _ in range(50):
        if socket.exists():
            break
        time.sleep(0.1)
    subprocess.run(["ssh-add", str(root / "id_ed25519")], check=True, env=env,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    result = subprocess.call(["cargo", "test", "-p", "axdb", "--", "--test-threads=1"], env=env)
finally:
    agent.terminate()
    agent.wait(timeout=10)
raise SystemExit(result)
