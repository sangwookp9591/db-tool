#!/usr/bin/env -S uv run --script
# /// script
# dependencies = ["msgpack>=1.1,<2"]
# ///
"""macOS driver-only paired legacy/stream benchmark; uses isolated local test DBs."""
import argparse
import ctypes
import hashlib
import io
import json
import platform
import statistics
import struct
import subprocess
import threading
import time
from pathlib import Path

import msgpack


class Usage(ctypes.Structure):
    _fields_ = [("uuid", ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64) for name in (
            "user", "system", "idle", "interrupt", "pageins", "wired", "rss", "footprint",
            "start", "exit", "child_user", "child_system", "child_idle", "child_interrupt",
            "child_pageins", "child_elapsed", "disk_read", "disk_write",
        )
    ]


def read_exact(pipe, size):
    data = bytearray()
    while len(data) < size:
        chunk = pipe.read(size - len(data))
        if not chunk:
            raise RuntimeError("driver closed its output")
        data.extend(chunk)
    return data


def read_frame(pipe):
    size, = struct.unpack(">I", read_exact(pipe, 4))
    assert 0 < size <= 16 * 1024 * 1024
    return msgpack.unpackb(read_exact(pipe, size), raw=False)


def send(pipe, ident, method, params):
    data = msgpack.packb({"id": ident, "method": method, "params": params}, use_bin_type=True)
    pipe.write(struct.pack(">I", len(data)) + data)
    pipe.flush()


def reply(pipe):
    message = read_frame(pipe)
    if "error" in message:
        raise RuntimeError("driver request failed: " + message["error"]["kind"])
    return message["result"]


def query(rows):
    digits = "(SELECT 0 n UNION ALL SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5 UNION ALL SELECT 6 UNION ALL SELECT 7 UNION ALL SELECT 8 UNION ALL SELECT 9)"
    sql = f"SELECT a.n+10*b.n+100*c.n+1000*d.n+10000*e.n AS n, 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' AS text FROM {digits} a CROSS JOIN {digits} b CROSS JOIN {digits} c CROSS JOIN {digits} d CROSS JOIN {digits} e"
    assert rows in (100000, 1000000)
    return "; ".join([sql] * (rows // 100000))


def measure(binary, database, rows, method, slow_ms=0):
    port = {"mysql": 53306, "mariadb": 53307}[database]
    child = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    done = threading.Event()
    peak = {"rss": 0, "footprint": 0, "samples": 0}
    lib = ctypes.CDLL("/usr/lib/libproc.dylib")
    lib.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]

    def sample():
        while not done.is_set():
            usage = Usage()
            if lib.proc_pid_rusage(child.pid, 2, ctypes.byref(usage)) == 0:
                peak["rss"] = max(peak["rss"], usage.rss)
                peak["footprint"] = max(peak["footprint"], usage.footprint)
                peak["samples"] += 1
            done.wait(0.002)

    sampler = threading.Thread(target=sample)
    try:
        send(child.stdin, 1, "handshake", {"protocol": 1})
        handshake = reply(child.stdout)["value"]
        assert handshake["protocol"] == 1
        if method == "execute_stream":
            assert handshake["manifest"]["capabilities"]["execute_stream"]
        send(child.stdin, 2, "connect", {"fields": {
            "host": "127.0.0.1", "port": str(port), "database": "axdb_test", "user": "root", "sslmode": "disable",
        }, "secret": "testmysql", "connect_timeout_ms": 5000})
        session = reply(child.stdout)["value"]["session"]
        send(child.stdin, 3, "server_info", {"session": session})
        version = reply(child.stdout)["value"]
        sampler.start()
        started = time.perf_counter()
        send(child.stdin, 4, method, {"session": session, "req": {"text": query(rows), "row_limit": 100000, "timeout_ms": 120000, "read_only": True}})
        first = None
        count = 0
        starts = []
        ends = []
        while True:
            message = read_frame(child.stdout)
            if "error" in message:
                raise RuntimeError("execution failed: " + message["error"]["kind"])
            if "result" in message:
                assert message["result"]["type"] == "unit"
                break
            chunk = message["chunk"]
            kind = chunk["type"]
            if kind in ("start", "stream_start"):
                starts.append(chunk["result"]["row_count"])
            if kind == "stream_end":
                assert not chunk["result"]["truncated"]
                ends.append(chunk["result"]["row_count"])
            if kind == "rows":
                if first is None:
                    first = (time.perf_counter() - started) * 1000
                count += len(chunk["rows"])
                if slow_ms:
                    time.sleep(slow_ms / 1000)
        total = (time.perf_counter() - started) * 1000
        assert count == rows
        assert len(starts) == rows // 100000
        if method == "execute_stream":
            assert starts == [0] * len(starts)
            assert sum(ends) == rows
        assert peak["samples"] > 0 and first is not None
        return {"database": database, "version": version, "rows": rows, "result_sets": rows // 100000,
                "method": method, "slow_consumer_ms": slow_ms, "peak_rss_mib": peak["rss"] / 2**20,
                "peak_footprint_mib": peak["footprint"] / 2**20, "first_rows_ms": first,
                "total_ms": total, "memory_samples": peak["samples"]}
    finally:
        done.set()
        if sampler.is_alive():
            sampler.join()
        child.kill()
        child.wait()
        child.stdin.close()
        child.stdout.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target-l3/debug/axdb-driver-mysql"))
    parser.add_argument("--pairs", type=int, default=3)
    parser.add_argument("--baseline", type=Path, help="Optional saved pre-change debug driver for legacy execute")
    parser.add_argument("--output", type=Path, default=Path("bench/results-l3-streaming.json"))
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    assert ctypes.sizeof(Usage) == 160
    assert query(1000000).count(" AS text") == 10
    assert msgpack.unpackb(msgpack.packb({"n": 18446744073709551615})) == {"n": 18446744073709551615}
    if args.self_check:
        pipe = io.BytesIO()
        send(pipe, 7, "ping", {"session": 3})
        class Fragmented(io.BytesIO):
            def read(self, size):
                return super().read(min(size, 2))
        assert read_frame(Fragmented(pipe.getvalue())) == {"id": 7, "method": "ping", "params": {"session": 3}}
        try:
            read_frame(io.BytesIO(b"\x00\x00\x00\x01"))
        except RuntimeError:
            pass
        else:
            raise AssertionError("truncated frame accepted")
        print("benchmark self-check passed")
        return
    assert platform.system() == "Darwin" and args.pairs > 0
    binary = args.binary.resolve(strict=True)
    baseline = args.baseline.resolve(strict=True) if args.baseline else binary
    samples = []
    for database in ("mysql", "mariadb"):
        for rows in (100000, 1000000):
            for pair in range(args.pairs):
                methods = ("execute", "execute_stream") if pair % 2 == 0 else ("execute_stream", "execute")
                for method in methods:
                    sample = measure(baseline if method == "execute" else binary, database, rows, method)
                    sample["pair"] = pair
                    samples.append(sample)
                    print(json.dumps(sample), flush=True)
    for database in ("mysql", "mariadb"):
        samples.append(measure(binary, database, 1000000, "execute_stream", slow_ms=2))
    medians = []
    for database in ("mysql", "mariadb"):
        for rows in (100000, 1000000):
            for method in ("execute", "execute_stream"):
                group = [s for s in samples if s["database"] == database and s["rows"] == rows and s["method"] == method and not s["slow_consumer_ms"]]
                medians.append({"database": database, "rows": rows, "method": method, **{
                    metric: statistics.median(s[metric] for s in group) for metric in ("peak_rss_mib", "peak_footprint_mib", "first_rows_ms", "total_ms")
                }})
    report = {"boundary": "debug driver stdio; excludes host/WebView/DB server", "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "baseline_sha256": hashlib.sha256(baseline.read_bytes()).hexdigest(),
              "comparison": "saved pre-change legacy / final streaming" if args.baseline else "same binary legacy / streaming",
              "memory": "proc_pid_rusage v2 at about 2ms; sampled peaks, not exact high-water marks", "pairs": args.pairs,
              "row_limit": "100000 per result; 1000000 total uses ten result sets", "platform": platform.platform(),
              "load_gate": "not applied; shared workspace may run concurrent builds", "samples": samples, "medians": medians}
    args.output.write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
