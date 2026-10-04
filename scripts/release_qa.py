#!/usr/bin/env python3
"""Verify public release downloads and run the installed executable on a native host."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile
import urllib.request
import zipfile
from package_release import PLATFORMS

ROOT = Path(__file__).resolve().parent.parent


def download(url, path):
    request = urllib.request.Request(url, headers={"User-Agent": "Resen-release-QA"})
    with urllib.request.urlopen(request, timeout=180) as response:
        data = response.read(33_554_433)
    if len(data) > 33_554_432:
        raise ValueError("Release download exceeds size limit")
    path.write_bytes(data)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--platform", choices=PLATFORMS, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[A-Za-z0-9.-]+)?", args.version):
        parser.error("Invalid version tag")
    base = f"https://github.com/desenyon/resen/releases/download/{args.version}"
    args.output.mkdir(parents=True, exist_ok=True)
    windows = args.platform.startswith("windows")
    name = f"resen-{args.platform}" + (".zip" if windows else ".tar.gz")
    with tempfile.TemporaryDirectory(prefix="resen-public-release-") as temp:
        root = Path(temp)
        sums = root / "SHA256SUMS"
        download(f"{base}/SHA256SUMS", sums)
        entries = [line.split() for line in sums.read_text().splitlines()]
        hashes = {name: digest for digest, name in entries}
        if len(hashes) != len(entries):
            raise ValueError("Duplicate checksum entries")
        archive = root / name
        download(f"{base}/{name}", archive)
        if hashlib.sha256(archive.read_bytes()).hexdigest() != hashes[name]:
            raise ValueError("Release archive checksum mismatch")
        if windows:
            with zipfile.ZipFile(archive) as package:
                metadata = json.loads(package.read("RELEASE.json"))
                binary = package.read("resen.exe")
            installed = args.output / "resen.exe"
            installed.write_bytes(binary)
        else:
            with tarfile.open(archive) as package:
                metadata = json.load(package.extractfile("RELEASE.json"))
                binary = package.extractfile("resen").read()
            script = root / "install.sh"
            download(f"{base}/install.sh", script)
            if hashlib.sha256(script.read_bytes()).hexdigest() != hashes["install.sh"]:
                raise ValueError("Installer checksum mismatch")
            if script.read_bytes() != (ROOT / "install.sh").read_bytes():
                raise ValueError("Published installer differs from checked-out tag")
            subprocess.run(["sh", str(script), "--version", args.version, "--dir", str(args.output.resolve())],
                           check=True, timeout=420)
            installed = args.output / "resen"
        revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        if metadata != {"version": args.version[1:], "platform": args.platform,
                        "target": PLATFORMS[args.platform], "revision": revision,
                        "binary_sha256": hashlib.sha256(binary).hexdigest()}:
            raise ValueError("Release metadata does not match platform, tag or binary")
        if installed.read_bytes() != binary:
            raise ValueError("Installed binary differs from verified archive")
        version = subprocess.check_output([str(installed.resolve()), "--version"], text=True, timeout=10).strip()
        if version != f"resen {args.version[1:]}":
            raise ValueError("Installed version mismatch")
        subprocess.run([str(installed.resolve()), "--help"], check=True, capture_output=True, timeout=10)
        result = subprocess.check_output([str(installed.resolve()), "--demo", "--data-dir", str(root / "state"),
                                          "backtest", "--symbol", "NVDA"], text=True, timeout=20)
        if not json.loads(result)["demo"]:
            raise ValueError("Offline study lost demo provenance")
        print(f"Public archive, checksum, installer, version/help and offline study verified: {args.platform} {version}")


if __name__ == "__main__":
    main()
