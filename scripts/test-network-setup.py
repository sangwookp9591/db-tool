"""Create only axdb-test-* fixtures; emit no credentials. Run from the repository root."""
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
import shutil
import time


def run(*args, **kwargs):
    return subprocess.run(args, check=True, stdout=subprocess.DEVNULL,
                          stderr=subprocess.DEVNULL, **kwargs)


def tls_certificates(tls):
    tls.mkdir(mode=0o700, exist_ok=True)
    run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
        "-keyout", str(tls / "ca.key"), "-out", str(tls / "ca.crt"),
        "-subj", "/CN=AxDB Test CA", "-days", "2")
    run("openssl", "req", "-new", "-newkey", "rsa:2048", "-nodes",
        "-keyout", str(tls / "server.key"), "-out", str(tls / "server.csr"),
        "-subj", "/CN=host.docker.internal", "-addext",
        "subjectAltName=DNS:host.docker.internal")
    (tls / "server.ext").write_text("subjectAltName=DNS:host.docker.internal\n"
                                    "extendedKeyUsage=serverAuth\nkeyUsage=digitalSignature,keyEncipherment\n"
                                    "basicConstraints=critical,CA:FALSE\n")
    run("openssl", "x509", "-req", "-in", str(tls / "server.csr"),
        "-CA", str(tls / "ca.crt"), "-CAkey", str(tls / "ca.key"), "-CAcreateserial",
        "-out", str(tls / "server.crt"), "-days", "2", "-extfile", str(tls / "server.ext"))
    os.chmod(tls / "ca.key", 0o600)
    os.chmod(tls / "server.key", 0o600)


def setup_tls(root):
    tls = root / "tls"
    tls_certificates(tls)
    env = dict(os.environ, POSTGRES_PASSWORD="testpw")
    run("docker", "run", "-d", "--name", "axdb-test-w1-pg-tls", "-p", "127.0.0.1:55433:5432",
        "-e", "POSTGRES_PASSWORD", "-e", "POSTGRES_DB=axdb_test",
        "-v", f"{tls}:/tls:ro", "--entrypoint", "sh", "postgres:16-alpine", "-c",
        "cp /tls/server.key /tmp/server.key; cp /tls/server.crt /tmp/server.crt; "
        "chown postgres:postgres /tmp/server.key /tmp/server.crt; chmod 600 /tmp/server.key; "
        "exec /usr/local/bin/docker-entrypoint.sh postgres -c ssl=on "
        "-c ssl_cert_file=/tmp/server.crt -c ssl_key_file=/tmp/server.key", env=env)
    for _ in range(60):
        ready = subprocess.run(["docker", "exec", "axdb-test-w1-pg-tls", "pg_isready", "-h", "127.0.0.1", "-U", "postgres"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if ready.returncode == 0:
            fixture_path = root / "fixture.json"
            fixture = json.loads(fixture_path.read_text())
            fixture["tls_ca"] = str(tls / "ca.crt")
            fixture_path.write_text(json.dumps(fixture))
            return
        time.sleep(0.5)
    raise RuntimeError("TLS PostgreSQL test fixture did not become ready")


def main():
    root = Path(tempfile.mkdtemp(prefix="axdb-w1-network-"))
    os.chmod(root, 0o700)
    run("ssh-keygen", "-t", "ed25519", "-N", "", "-f", str(root / "id_ed25519"))
    passphrase = secrets.token_urlsafe(24)
    shutil.copyfile(root / "id_ed25519", root / "id_encrypted")
    os.chmod(root / "id_encrypted", 0o600)
    run("ssh-keygen", "-p", "-P", "", "-N", passphrase, "-f", str(root / "id_encrypted"))
    password = secrets.token_urlsafe(24)
    env = dict(os.environ, USER_PASSWORD=password, PUBLIC_KEY=(root / "id_ed25519.pub").read_text().strip())
    # Do not adopt or stop any pre-existing container.
    run("docker", "run", "-d", "--name", "axdb-test-ssh", "-p", "127.0.0.1:55222:2222",
        "-e", "USER_NAME=axdbtest", "-e", "PASSWORD_ACCESS=true", "-e", "SUDO_ACCESS=false",
        "-e", "USER_PASSWORD", "-e", "PUBLIC_KEY", "lscr.io/linuxserver/openssh-server:latest", env=env)
    for _ in range(60):
        ready = subprocess.run(["docker", "exec", "axdb-test-ssh", "sh", "-c",
                                "test -f /config/sshd/sshd_config && test -f /config/sshd.pid"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if ready.returncode == 0:
            break
        time.sleep(0.5)
    else:
        raise RuntimeError("SSH test fixture did not become ready")
    run("docker", "exec", "axdb-test-ssh", "sh", "-c",
        'sed -i "s/^AllowTcpForwarding no/AllowTcpForwarding yes/" /config/sshd/sshd_config; kill -HUP $(cat /config/sshd.pid)')
    script = str(Path("scripts/test-network-proxy.py").resolve())
    run("docker", "run", "-d", "--name", "axdb-test-socks", "-p", "127.0.0.1:55080:1080",
        "-v", f"{script}:/proxy.py:ro", "python:3.13-alpine", "python", "/proxy.py")
    fixture = {"key_file": str(root / "id_ed25519"), "encrypted_key_file": str(root / "id_encrypted"), "ssh_host": "127.0.0.1", "ssh_port": 55222,
               "ssh_user": "axdbtest", "proxy_host": "127.0.0.1", "proxy_port": 55080}
    (root / "fixture.json").write_text(json.dumps(fixture))
    (root / "secrets.json").write_text(json.dumps({"w1-network:ssh_password": password, "w1-network:key_passphrase": passphrase}))
    os.chmod(root / "fixture.json", 0o600)
    os.chmod(root / "secrets.json", 0o600)
    setup_tls(root)
    Path("/tmp/axdb-w1-network-fixture-path").write_text(str(root))
    print("Created axdb-test-ssh and axdb-test-socks; fixture path recorded privately in /tmp.")


if __name__ == "__main__":
    main()
