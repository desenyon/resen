#!/bin/sh
# Install a checksummed Resen release without sudo or a compiler.
set -eu

version=latest
install_dir=${HOME:?HOME must be set}/.local/bin
usage() {
    printf '%s\n' 'Usage: sh install.sh [--version v0.2.0] [--dir DIRECTORY]'
}
die() {
    printf 'resen installer: %s\n' "$1" >&2
    exit 1
}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --version) [ "$#" -ge 2 ] || die 'Missing version.'; version=$2; shift 2 ;;
        --dir) [ "$#" -ge 2 ] || die 'Missing installation directory.'; install_dir=$2; shift 2 ;;
        --help|-h) usage; exit 0 ;;
        *) usage >&2; die 'Unknown argument.' ;;
    esac
done
if [ "$version" != latest ]; then
    printf '%s\n' "$version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' || die 'Use a release tag such as v0.2.0.'
fi
[ -n "$install_dir" ] || die 'Installation directory cannot be empty.'
case "$install_dir" in /*) ;; *) install_dir=$PWD/$install_dir ;; esac
case "$(uname -s)" in
    Darwin) platform=macos ;;
    Linux) platform=linux ;;
    *) die 'This installer supports macOS and Linux. Windows ZIPs are on GitHub Releases.' ;;
esac
case "$(uname -m)" in
    arm64|aarch64) arch=arm64 ;;
    x86_64|amd64) arch=x86_64 ;;
    *) die 'Supported architectures are x86_64 and arm64.' ;;
esac
for tool in curl tar awk mktemp chmod mv; do
    command -v "$tool" >/dev/null 2>&1 || die "Required command is missing: $tool"
done
if command -v sha256sum >/dev/null 2>&1; then
    hash_tool=sha256sum
elif command -v shasum >/dev/null 2>&1; then
    hash_tool=shasum
else
    die 'A SHA-256 verifier is required (sha256sum or shasum).'
fi
asset=resen-$platform-$arch.tar.gz
base=https://github.com/desenyon/resen/releases
if [ "$version" = latest ]; then base=$base/latest/download; else base=$base/download/$version; fi
umask 077
tmp=$(mktemp -d "${TMPDIR:-/tmp}/resen-install.XXXXXX")
stage=
cleanup() {
    [ -z "$stage" ] || rm -f "$stage"
    rm -rf "$tmp"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
fetch() {
    curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
        --connect-timeout 10 --max-time 180 --max-filesize 33554432 \
        --retry 2 --retry-delay 1 --output "$2" "$1"
}
printf 'Downloading %s (%s)…\n' "$asset" "$version"
fetch "$base/$asset" "$tmp/archive.tar.gz" || die 'Download failed. No existing installation was changed.'
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die 'Checksum download failed.'
expected=$(awk -v name="$asset" '$2 == name { print $1 }' "$tmp/SHA256SUMS")
[ "${#expected}" -eq 64 ] || die 'Release checksum is missing or ambiguous.'
printf '%s\n' "$expected" | grep -Eq '^[a-f0-9]{64}$' || die 'Invalid release checksum.'
if [ "$hash_tool" = sha256sum ]; then
    actual=$(sha256sum "$tmp/archive.tar.gz" | awk '{print $1}')
else
    actual=$(shasum -a 256 "$tmp/archive.tar.gz" | awk '{print $1}')
fi
[ "$actual" = "$expected" ] || die 'Checksum mismatch. No existing installation was changed.'
# Stream only the exact binary member; never extract archive paths into the filesystem.
members=$(tar -tzf "$tmp/archive.tar.gz") || die 'Invalid release archive.'
count=$(printf '%s\n' "$members" | awk '$0 == "resen" {n++} END {print n+0}')
[ "$count" -eq 1 ] || die 'Archive must contain exactly one resen executable.'
mkdir -p "$install_dir" || die 'Cannot create the installation directory. Try --dir.'
[ ! -d "$install_dir/resen" ] && [ ! -L "$install_dir/resen" ] || die 'Destination is a directory or symlink; choose another --dir.'
stage=$(mktemp "$install_dir/.resen-install.XXXXXX") || die 'Installation directory is not writable. Try --dir.'
tar -xzOf "$tmp/archive.tar.gz" resen > "$stage" || die 'Could not read the release executable.'
[ -s "$stage" ] || die 'Release executable is empty.'
chmod 755 "$stage"
installed_version=$("$stage" --version) || die 'Executable cannot run on this system. Existing installation preserved.'
if [ "$version" != latest ]; then
    [ "$installed_version" = "resen ${version#v}" ] || die 'Executable version does not match the requested release.'
else
    printf '%s\n' "$installed_version" | grep -Eq '^resen [0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$' || die 'Unexpected executable version.'
fi
"$stage" --help >/dev/null || die 'Executable smoke check failed. Existing installation preserved.'
mv -f "$stage" "$install_dir/resen"
stage=
printf 'Installed %s to %s/resen\n' "$installed_version" "$install_dir"
printf 'Try: "%s/resen" --demo\n' "$install_dir"
case :$PATH: in
    *:"$install_dir":*) printf '%s\n' 'You can also run: resen --demo' ;;
    *) printf 'Add this directory to your PATH to use the resen command: %s\n' "$install_dir" ;;
esac
