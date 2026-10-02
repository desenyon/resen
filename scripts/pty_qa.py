#!/usr/bin/env python3
"""Exercise the actual executable through a Unix pseudo-terminal; stdlib only."""
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default="target/debug/resen")
    parser.add_argument("--output", default=".qa/pty-report.json")
    args = parser.parse_args()
    binary = str(Path(args.binary).resolve())
    results = []
    with tempfile.TemporaryDirectory(prefix="resen-pty-") as temp:
        master, slave = pty.openpty()
        original = termios.tcgetattr(slave)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 46, 144, 0, 0))
        env = dict(os.environ, TERM="xterm-256color", COLORTERM="truecolor")
        process = subprocess.Popen(
            [binary, "--demo", "--data-dir", temp],
            stdin=slave, stdout=slave, stderr=slave, env=env,
        )
        captured = bytearray()

        def read(duration=0.25):
            deadline = time.monotonic() + duration
            while time.monotonic() < deadline:
                readable, _, _ = select.select([master], [], [], min(0.05, max(0, deadline-time.monotonic())))
                if readable:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError:
                        break
                    if not chunk:
                        break
                    captured.extend(chunk)

        def send(value, duration=0.25):
            os.write(master, value)
            read(duration)

        def check(condition, name):
            if not condition:
                raise AssertionError(name)
            results.append(name)

        try:
            read(0.8)
            check(process.poll() is None, "Executable enters interactive terminal")
            check(b"\x1b[?1049h" in captured, "Alternate screen enabled")
            check(b"\x1b[?2004h" in captured, "Bracketed paste enabled")
            send(b"n\t\x15")
            send(b"\x1b[200~AAPL\x1b[201~\t\x15")
            send("\x1b[200~Assess cash flows and durability. 研究\x1b[201~".encode())
            send(b"\x12", 1.6)  # Ctrl+R works without enhanced keyboard support.
            db = sqlite3.connect(Path(temp) / "research.db")
            rows = db.execute("SELECT body FROM runs ORDER BY created_at DESC").fetchall()
            check(len(rows) == 1, "Research submission creates exactly one run")
            run = json.loads(rows[0][0])
            check(run["status"] == "complete", "Demo research completes through real terminal input")
            check(run["request"]["symbols"] == ["AAPL"], "Bracketed paste updates the asset field")
            check("研究" in run["request"]["question"], "Unicode question is preserved")
            check(run["demo"] and run["sources"], "Demo source ledger is attached")
            send(b"e")
            check((Path(temp)/"exports"/f"{run['id']}.md").exists(), "Export key writes a memo")
            send(b"3\x1b[6~5/\x1b[200~AAPL\x1b[201~\r\r")
            check(process.poll() is None, "Sources, scrolling and archive search remain responsive")
            send(b"1\x0b")  # Ctrl+K
            send(b"historical\r")
            send(b"\r")
            check((Path(temp)/"exports"/"latest-strategy.json").exists(), "Palette runs a historical strategy and exports results")
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
            process.send_signal(signal.SIGWINCH)
            read(0.3)
            send(b"?\x1b")
            check(process.poll() is None, "Resize and help dialog do not crash")
            send(b"n\x12", 0.1)
            send(b"\x03", 0.4)
            rows = db.execute("SELECT body FROM runs ORDER BY created_at DESC").fetchall()
            latest = json.loads(rows[0][0])
            check(latest["status"] == "cancelled", "Ctrl+C cancels an active job and preserves it")
            check(process.poll() is None, "Cancelling research leaves the desk open")
            send(b"q", 0.5)
            process.wait(timeout=5)
            read(0.1)
            check(process.returncode == 0, "Normal quit exits successfully")
            check(b"\x1b[?1049l" in captured, "Alternate screen restored")
            check(b"\x1b[?2004l" in captured, "Bracketed paste disabled on exit")
            check(termios.tcgetattr(slave) == original, "Terminal attributes restored exactly")
            db.close()
            for interrupt in [signal.SIGTERM, signal.SIGINT]:
                name = signal.Signals(interrupt).name
                captured.clear()
                process = subprocess.Popen(
                    [binary, "--demo", "--data-dir", temp],
                    stdin=slave, stdout=slave, stderr=slave, env=env,
                )
                read(0.5)
                send(b"n\x12", 0.1)
                process.send_signal(interrupt)
                deadline = time.monotonic() + 5
                while process.poll() is None and time.monotonic() < deadline:
                    read(0.05)
                try:
                    process.wait(timeout=1)
                except subprocess.TimeoutExpired as error:
                    failure = Path(args.output).with_suffix(".failed.log")
                    failure.parent.mkdir(parents=True, exist_ok=True)
                    failure.write_bytes(captured)
                    raise AssertionError(f"{name} did not exit; terminal capture: {failure}") from error
                read(0.1)
                check(process.returncode == 0, f"{name} exits successfully during research")
                check(b"\x1b[?1049l" in captured and b"\x1b[?2004l" in captured,
                      f"{name} restores screen and paste mode")
                check(termios.tcgetattr(slave) == original, f"{name} restores terminal attributes exactly")
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            os.close(master)
            os.close(slave)
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({"passed": len(results), "checks": results}, indent=2)+"\n")
    print(f"PTY QA: {len(results)} checks passed. {output}")


if __name__ == "__main__":
    main()
