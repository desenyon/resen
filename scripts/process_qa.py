#!/usr/bin/env python3
"""Verify cancellation of a real child process without contacting a model."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import sqlite3
import struct
import subprocess
import tempfile
import termios
import time


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    # An exited child awaiting reaping is no longer doing work.
    try:
        status = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)],
                                capture_output=True, text=True, timeout=2).stdout.strip()
    except PermissionError:
        return True
    return bool(status) and not status.startswith("Z")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/resen")
    parser.add_argument("--output", default=".qa/process-report.json")
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    checks = []

    def check(condition, name):
        if not condition:
            raise AssertionError(name)
        checks.append(name)

    with tempfile.TemporaryDirectory(prefix="resen-process-") as directory:
        root = Path(directory)
        fakebin = root / "bin"
        fakebin.mkdir()
        codex = fakebin / "codex"
        codex.write_text("""#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
assert sys.argv[1] == 'exec'
assert 'features.shell_tool=false' in sys.argv
assert 'web_search="disabled"' in sys.argv
assert 'UNTRUSTED SOURCE LEDGER' in sys.stdin.read()
Path(os.environ['RESEN_QA_CHILD_PID']).write_text(str(os.getpid()))
print(json.dumps({'type': 'item.completed', 'item': {'type': 'agent_message', 'text': 'Checkpoint fixture partial memo [1].'}}), flush=True)
time.sleep(60)
""")
        codex.chmod(0o700)
        env = dict(os.environ, PATH=str(fakebin) + os.pathsep + os.environ.get("PATH", ""),
                   TERM="xterm-256color", COLORTERM="truecolor")
        for key in ["ALPHAVANTAGE_API_KEY", "STOOQ_API_KEY", "BRAVE_API_KEY",
                    "TAVILY_API_KEY", "FRED_API_KEY"]:
            env.pop(key, None)
        fixture = root / "synthetic-qa.csv"
        fixture.write_text("Date,Open,High,Low,Close,Volume\n"
                           "2025-01-02,100,102,99,101,1000\n"
                           "2025-01-03,101,103,100,102,1000\n")
        for mode in ["headless-SIGINT", "headless-SIGTERM", "headless-SIGKILL", "terminal-quit", "terminal-SIGTERM"]:
            state = root / mode
            state.mkdir()
            state.joinpath("config.toml").write_text(
                'provider = "codex"\nmodel = ""\nendpoint = ""\n'
                'data_provider = "csv"\nsetup_complete = true\nwatchlist = ["TEST"]\n')
            subprocess.run([binary, "--data-dir", str(state), "import-csv", str(fixture),
                            "--symbol", "TEST"], env=env, check=True,
                           capture_output=True, timeout=10)
            marker = root / (mode + ".pid")
            env["RESEN_QA_CHILD_PID"] = str(marker)
            terminal = mode.startswith("terminal")
            master = slave = None
            child_pid = None
            capture = bytearray()
            if terminal:
                master, slave = pty.openpty()
                original = termios.tcgetattr(slave)
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 46, 144, 0, 0))
                process = subprocess.Popen([binary, "--data-dir", str(state)],
                                           stdin=slave, stdout=slave, stderr=slave, env=env)
            else:
                process = subprocess.Popen([binary, "--data-dir", str(state), "research",
                                           "Describe the synthetic QA fixture", "--symbols", "TEST"],
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)

            def drain():
                if terminal and select.select([master], [], [], 0.02)[0]:
                    try:
                        capture.extend(os.read(master, 65536))
                    except OSError:
                        pass
                else:
                    time.sleep(0.02)

            try:
                if terminal:
                    deadline = time.monotonic() + 1
                    while time.monotonic() < deadline:
                        drain()
                    os.write(master, b"n\x12")
                deadline = time.monotonic() + 10
                while not marker.exists() and process.poll() is None and time.monotonic() < deadline:
                    drain()
                check(marker.exists(), f"{mode}: native provider process starts")
                child_pid = int(marker.read_text())
                check(alive(child_pid), f"{mode}: provider is running before cancellation")
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    with sqlite3.connect(state / "research.db") as db:
                        checkpoint = json.loads(db.execute("SELECT body FROM runs").fetchone()[0])
                    if "Checkpoint fixture partial memo" in checkpoint["report"]:
                        break
                    drain()
                check(checkpoint["status"] == "running" and "Checkpoint fixture partial memo" in checkpoint["report"],
                      f"{mode}: stalled stream is checkpointed before termination")
                if mode == "headless-SIGKILL":
                    process.kill()
                elif mode == "terminal-quit":
                    os.write(master, b"q")
                else:
                    process.send_signal(signal.SIGTERM if mode.endswith("SIGTERM") else signal.SIGINT)
                deadline = time.monotonic() + 8
                while process.poll() is None and time.monotonic() < deadline:
                    drain()
                process.wait(timeout=1)
                check((process.returncode == 0) if terminal else (process.returncode != 0),
                      f"{mode}: application exits with the expected status")
                if mode == "headless-SIGKILL":
                    # SIGKILL cannot run child cleanup. Stop this known fixture child.
                    if alive(child_pid):
                        os.kill(child_pid, signal.SIGKILL)
                    output = subprocess.run([binary, "--data-dir", str(state), "history", "--json"],
                                            env=env, check=True, capture_output=True, text=True, timeout=10)
                    recovered = json.loads(output.stdout)["runs"][0]
                    check(recovered["status"] == "interrupted", f"{mode}: next launch recovers interrupted research")
                deadline = time.monotonic() + 2
                while alive(child_pid) and time.monotonic() < deadline:
                    time.sleep(0.05)
                check(not alive(child_pid), f"{mode}: cancelled provider stops")
                with sqlite3.connect(state / "research.db") as db:
                    run = json.loads(db.execute("SELECT body FROM runs").fetchone()[0])
                expected = "interrupted" if mode == "headless-SIGKILL" else "cancelled"
                check(run["status"] == expected and run["sources"] and "Checkpoint fixture partial memo" in run["report"],
                      f"{mode}: termination preserves the source ledger and partial memo")
                if terminal:
                    for _ in range(4):
                        drain()
                    check(termios.tcgetattr(slave) == original,
                          f"{mode}: terminal attributes are restored")
                    check(b"\x1b[?1049l" in capture and b"\x1b[?2004l" in capture,
                          f"{mode}: screen and paste mode are restored")
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait(timeout=5)
                if child_pid and alive(child_pid):
                    os.kill(child_pid, signal.SIGKILL)
                if terminal:
                    os.close(master)
                    os.close(slave)
                else:
                    process.communicate(timeout=2)
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({"passed": len(checks), "checks": checks}, indent=2) + "\n")
    print(f"Process QA: {len(checks)} checks passed. {output}")


if __name__ == "__main__":
    main()
