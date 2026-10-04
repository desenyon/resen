#!/usr/bin/env python3
"""Package native release executables and collect verified versioned artifacts."""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
PLATFORMS = {
    "linux-x86_64": "x86_64-unknown-linux-musl",
    "linux-arm64": "aarch64-unknown-linux-musl",
    "macos-x86_64": "x86_64-apple-darwin",
    "macos-arm64": "aarch64-apple-darwin",
    "windows-x86_64": "x86_64-pc-windows-msvc",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def release_files(binary, platform):
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    output = subprocess.check_output([str(binary.resolve()), "--version"], text=True, timeout=10).strip()
    if output != f"resen {version}":
        raise ValueError("Executable version does not match Cargo.toml")
    subprocess.run([str(binary.resolve()), "--help"], check=True, capture_output=True, timeout=10)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    name = "resen.exe" if platform.startswith("windows") else "resen"
    data = binary.read_bytes()
    metadata = {"version": version, "platform": platform, "target": PLATFORMS[platform],
                "revision": revision, "binary_sha256": digest(data)}
    return {name: data, "LICENSE": (ROOT / "LICENSE").read_bytes(),
            "README.md": (ROOT / "README.md").read_bytes(),
            "CHANGELOG.md": (ROOT / "CHANGELOG.md").read_bytes(),
            "RELEASE.json": (json.dumps(metadata, indent=2) + "\n").encode()}


def package(binary, platform, output):
    files = release_files(binary, platform)
    output.mkdir(parents=True, exist_ok=True)
    suffix = ".zip" if platform.startswith("windows") else ".tar.gz"
    path = output / f"resen-{platform}{suffix}"
    if suffix == ".zip":
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, data in files.items():
                info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = (0o755 if name == "resen.exe" else 0o644) << 16
                archive.writestr(info, data)
    else:
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w") as archive:
            for name, data in files.items():
                info = tarfile.TarInfo(name)
                info.size = len(data)
                info.mode = 0o755 if name == "resen" else 0o644
                archive.addfile(info, io.BytesIO(data))
        path.write_bytes(gzip.compress(buffer.getvalue(), mtime=0))
    print(f"Packaged {path.name}: {digest(path.read_bytes())}")


def collect(source, output):
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    output.mkdir(parents=True, exist_ok=True)
    files = []
    for platform in PLATFORMS:
        suffix = ".zip" if platform.startswith("windows") else ".tar.gz"
        name = f"resen-{platform}{suffix}"
        matches = list(source.rglob(name))
        if len(matches) != 1:
            raise ValueError(f"Expected exactly one artifact: {name}")
        path = matches[0]
        if suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                metadata = json.loads(archive.read("RELEASE.json"))
                binary = archive.read("resen.exe")
        else:
            with tarfile.open(path) as archive:
                metadata = json.load(archive.extractfile("RELEASE.json"))
                binary = archive.extractfile("resen").read()
        expected = {"version": version, "platform": platform, "target": PLATFORMS[platform],
                    "revision": revision, "binary_sha256": digest(binary)}
        if metadata != expected:
            raise ValueError(f"Artifact version, source or binary digest mismatch: {name}")
        destination = output / name
        shutil.copyfile(path, destination)
        files.append(destination)
    installer = output / "install.sh"
    shutil.copyfile(ROOT / "install.sh", installer)
    files.append(installer)
    sums = "".join(f"{digest(p.read_bytes())}  {p.name}\n" for p in sorted(files))
    (output / "SHA256SUMS").write_text(sums)
    print(f"Collected {len(PLATFORMS)} native archives and installer; source {revision}, version {version}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--platform", choices=PLATFORMS)
    parser.add_argument("--collect", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.collect:
        collect(args.collect, args.output)
    elif args.binary and args.platform:
        package(args.binary, args.platform, args.output)
    else:
        parser.error("Choose --collect or --binary with --platform")
