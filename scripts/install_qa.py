#!/usr/bin/env python3
"""Hermetic installer failures and an optional real-binary installation; stdlib only."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
VERSION = "v0.2.2"
PAYLOAD = b'#!/bin/sh\ncase "$1" in --version) echo "resen 0.2.2";; --help) echo "Resen help";; *) exit 1;; esac\n'


def archive(path, payload=PAYLOAD, names=None):
    with tarfile.open(path, "w:gz") as tar:
        for name, data in names or [("resen", payload)]:
            member = tarfile.TarInfo(name)
            member.size = len(data)
            member.mode = 0o755
            tar.addfile(member, io.BytesIO(data))


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="resen-install-qa-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.fakebin = self.root / "fakebin"
        self.fakebin.mkdir()
        self.install_dir = self.root / "install with spaces"
        self.home = self.root / "home"
        self.home.mkdir()
        self.sentinel = self.home / "research-state"
        self.sentinel.write_text("existing state stays intact")
        self.asset = "resen-macos-arm64.tar.gz"
        self.env = dict(os.environ, HOME=str(self.home),
                        PATH=str(self.fakebin) + os.pathsep + os.environ["PATH"],
                        RESEN_INSTALL_QA_ROOT=str(self.root),
                        RESEN_INSTALL_QA_OS="Darwin", RESEN_INSTALL_QA_ARCH="arm64")
        self.fakebin.joinpath("uname").write_text(
            '#!/bin/sh\ncase "$1" in -s) echo "$RESEN_INSTALL_QA_OS";; -m) echo "$RESEN_INSTALL_QA_ARCH";; esac\n')
        self.fakebin.joinpath("curl").write_text(f"#!{sys.executable}\n" + r'''
import json, os, sys
from pathlib import Path
root = Path(os.environ['RESEN_INSTALL_QA_ROOT'])
args = sys.argv[1:]
assert '--proto' in args and args[args.index('--proto') + 1] == '=https'
assert '--connect-timeout' in args and '--max-time' in args and '--max-filesize' in args
url = args[-1]
assert url.startswith('https://github.com/desenyon/resen/releases/')
with (root / 'requests.jsonl').open('a') as log:
    log.write(json.dumps(url) + '\n')
if os.environ.get('RESEN_INSTALL_QA_FAIL_DOWNLOAD'):
    sys.exit(22)
source = root / url.rsplit('/', 1)[-1]
if not source.exists():
    sys.exit(22)
Path(args[args.index('--output') + 1]).write_bytes(source.read_bytes())
''')
        for file in self.fakebin.iterdir():
            file.chmod(0o755)
        self.make_release()

    def make_release(self, payload=PAYLOAD, names=None):
        path = self.root / self.asset
        archive(path, payload, names)
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        (self.root / "SHA256SUMS").write_text(f"{digest}  {self.asset}\n")

    def run_install(self, *args, success=True):
        result = subprocess.run(["sh", str(ROOT / "install.sh"), "--dir", str(self.install_dir), *args],
                                input="", capture_output=True, text=True, env=self.env, timeout=15)
        self.assertEqual(result.returncode == 0, success, result.stderr)
        self.assertEqual(self.sentinel.read_text(), "existing state stays intact")
        self.assertFalse(list(self.install_dir.glob(".resen-install.*")))
        return result

    def existing(self):
        self.install_dir.mkdir()
        path = self.install_dir / "resen"
        path.write_bytes(b"previous executable")
        return path

    def test_latest_install_and_repeat_are_idempotent(self):
        self.run_install()
        self.assertEqual((self.install_dir / "resen").read_bytes(), PAYLOAD)
        self.run_install()
        self.assertTrue(os.access(self.install_dir / "resen", os.X_OK))
        self.assertIn("/latest/download/", (self.root / "requests.jsonl").read_text())

    def test_pinned_version_and_relative_directory(self):
        self.run_install("--version", VERSION)
        self.assertIn(f"/download/{VERSION}/", (self.root / "requests.jsonl").read_text())
        relative = os.path.relpath(self.install_dir, Path.cwd())
        self.run_install("--dir", relative)

    def test_all_unix_platform_names_and_architecture_aliases(self):
        for system, machine, asset in [
            ("Darwin", "x86_64", "macos-x86_64"), ("Darwin", "arm64", "macos-arm64"),
            ("Linux", "x86_64", "linux-x86_64"), ("Linux", "aarch64", "linux-arm64"),
            ("Linux", "amd64", "linux-x86_64"), ("Linux", "arm64", "linux-arm64"),
        ]:
            with self.subTest(system=system, machine=machine):
                self.env.update(RESEN_INSTALL_QA_OS=system, RESEN_INSTALL_QA_ARCH=machine)
                self.asset = f"resen-{asset}.tar.gz"
                self.make_release()
                self.run_install()

    def test_download_failure_preserves_previous_install(self):
        old = self.existing()
        self.env["RESEN_INSTALL_QA_FAIL_DOWNLOAD"] = "1"
        self.run_install(success=False)
        self.assertEqual(old.read_bytes(), b"previous executable")

    def test_corrupted_checksum_preserves_previous_install(self):
        old = self.existing()
        (self.root / "SHA256SUMS").write_text(f"{'0' * 64}  {self.asset}\n")
        self.run_install(success=False)
        self.assertEqual(old.read_bytes(), b"previous executable")

    def test_missing_or_duplicate_checksum_is_rejected(self):
        for content in ["", (self.root / "SHA256SUMS").read_text() * 2]:
            with self.subTest(content=bool(content)):
                (self.root / "SHA256SUMS").write_text(content)
                self.run_install(success=False)

    def test_wrong_executable_version_preserves_previous_install(self):
        old = self.existing()
        self.make_release(PAYLOAD.replace(b"0.2.2", b"0.1.0"))
        self.run_install("--version", VERSION, success=False)
        self.assertEqual(old.read_bytes(), b"previous executable")

    def test_failed_smoke_check_preserves_previous_install(self):
        old = self.existing()
        self.make_release(PAYLOAD.replace(b'echo "Resen help"', b'exit 1'))
        self.run_install(success=False)
        self.assertEqual(old.read_bytes(), b"previous executable")

    def test_missing_or_duplicate_executable_is_rejected(self):
        for names in [[("other", PAYLOAD)], [("resen", PAYLOAD), ("resen", PAYLOAD)], [("resen", b"")]]:
            with self.subTest(names=len(names)):
                self.make_release(names=names)
                self.run_install(success=False)

    def test_archive_paths_are_never_extracted(self):
        self.make_release(names=[("resen", PAYLOAD), ("../../escape", b"not executable")])
        self.run_install()
        self.assertFalse((self.root / "escape").exists())
        self.assertFalse((self.home / "escape").exists())

    def test_destination_directory_and_symlink_are_not_overwritten(self):
        self.install_dir.mkdir()
        destination = self.install_dir / "resen"
        destination.mkdir()
        self.run_install(success=False)
        destination.rmdir()
        destination.symlink_to(self.sentinel)
        self.run_install(success=False)
        self.assertTrue(destination.is_symlink())

    def test_invalid_inputs_do_not_download(self):
        for args in [("--version", "../../bad"), ("--dir", ""), ("--unknown",), ("--version",)]:
            with self.subTest(args=args):
                self.run_install(*args, success=False)
        self.assertFalse((self.root / "requests.jsonl").exists())

    def test_unsupported_platform_does_not_download(self):
        for system, arch in [("Linux", "riscv64"), ("MINGW64_NT", "x86_64")]:
            self.env.update(RESEN_INSTALL_QA_OS=system, RESEN_INSTALL_QA_ARCH=arch)
            self.run_install(success=False)
        self.assertFalse((self.root / "requests.jsonl").exists())

    def test_help_has_no_network_or_install_side_effects(self):
        self.run_install("--help")
        self.assertFalse(self.install_dir.exists())
        self.assertFalse((self.root / "requests.jsonl").exists())


def real_binary_check(binary):
    fixture = InstallerTests()
    fixture.setUp()
    try:
        binary = Path(binary).resolve()
        version = subprocess.check_output([str(binary), "--version"], text=True, timeout=10).strip()
        tag = "v" + version.removeprefix("resen ")
        system = platform.system()
        machine = platform.machine().lower()
        name = "macos" if system == "Darwin" else "linux"
        arch = "arm64" if machine in ["arm64", "aarch64"] else "x86_64"
        fixture.env.update(RESEN_INSTALL_QA_OS=system, RESEN_INSTALL_QA_ARCH=machine)
        fixture.asset = f"resen-{name}-{arch}.tar.gz"
        fixture.make_release(binary.read_bytes())
        fixture.run_install("--version", tag)
        installed = fixture.install_dir / "resen"
        assert installed.read_bytes() == binary.read_bytes()
        subprocess.run([str(installed), "--demo", "--data-dir", str(fixture.root / "state"),
                        "backtest", "--symbol", "NVDA"], check=True, capture_output=True, timeout=15)
        print(f"Real executable installation and offline study: {version}")
    finally:
        fixture.doCleanups()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary")
    args = parser.parse_args()
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(InstallerTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if not result.wasSuccessful():
        sys.exit(1)
    if args.binary:
        real_binary_check(args.binary)
