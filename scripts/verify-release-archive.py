#!/usr/bin/env python3
"""Purpose: Verify a packaged release archive is complete and authentic.

Responsibilities:
- Check that every advertised binary is present in the archive for the
  given platform (testaruda-h7bv.2): engine + four adapters, `.exe`-suffixed
  on windows_amd64.
- Check Unix archive members carry the executable bit (DDL-3av regression
  guard: upload-artifact strips permissions).
- Check the archive's sha256 matches its checksums.txt entry.

Rationale: Nothing else inspects the artifact between packaging and
publication; a regression here ships silently. Exit non-zero on any
defect with a named, actionable reason so the release workflow aborts
before the GitHub release is created.
"""

import argparse
import hashlib
import sys
import tarfile
import zipfile
from pathlib import Path

# Every binary the README advertises as shipped in release archives.
ADVERTISED_BINARIES = [
    "testaruda",
    "testaruda-adapter-rust",
    "testaruda-adapter-python",
    "testaruda-adapter-typescript",
    "testaruda-adapter-clojure",
]

# Platform -> (archive basename suffix, member naming scheme).
PLATFORMS = {
    "linux_amd64": ("tar.gz", "unix"),
    "linux_arm64": ("tar.gz", "unix"),
    "darwin_amd64": ("tar.gz", "unix"),
    "darwin_arm64": ("tar.gz", "unix"),
    "windows_amd64": ("zip", "windows"),
}


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def tar_members(path):
    """Yield (name, executable) for each regular-file member of a tar.gz."""
    with tarfile.open(path, "r:gz") as tar:
        for info in tar.getmembers():
            if info.isfile():
                yield info.name, bool(info.mode & 0o111)


def zip_members(path):
    """Yield (name, executable=True) for each member of a Windows zip.

    Windows binaries have no POSIX mode; presence and naming are the
    contract, executability is meaningless on the target platform.
    """
    with zipfile.ZipFile(path, "r") as z:
        for info in z.infolist():
            if not info.is_dir():
                yield info.filename, True


def read_checksums(path):
    """Parse sha256sum-format lines into {filename: sha256}."""
    entries = {}
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        parts = line.split(None, 1)
        if len(parts) != 2:
            continue
        entries[Path(parts[1].strip().lstrip("*")).name] = parts[0].lower()
    return entries


def verify(archive, checksums_path, platform):
    """Return a list of defect strings; empty means the archive is good."""
    defects = []
    if platform not in PLATFORMS:
        return [f"unknown platform '{platform}' (known: {', '.join(PLATFORMS)})"]

    suffix, scheme = PLATFORMS[platform]
    if not archive.name.endswith(suffix):
        defects.append(
            f"archive suffix mismatch: expected *.{suffix} for {platform}")
    member_names = {name + (".exe" if scheme == "windows" else "")
                    for name in ADVERTISED_BINARIES}

    if not archive.exists():
        defects.append(f"archive not found: {archive}")
        return defects

    members = dict(tar_members(archive) if suffix == "tar.gz"
                   else zip_members(archive))

    for expected in sorted(member_names):
        if expected not in members:
            defects.append(f"missing binary: {expected}")
        elif suffix == "tar.gz" and not members[expected]:
            defects.append(f"missing exec bit: {expected}")

    extras = sorted(set(members) - member_names)
    if extras:
        defects.append(f"unexpected members: {', '.join(extras)}")

    entries = read_checksums(checksums_path)
    expected_sha = entries.get(archive.name)
    if expected_sha is None:
        defects.append(f"checksums.txt has no entry for {archive.name}")
    else:
        actual = sha256_of(archive)
        if actual != expected_sha:
            defects.append(
                f"checksum mismatch for {archive.name}: "
                f"expected {expected_sha}, got {actual}")
    return defects


def main():
    parser = argparse.ArgumentParser(
        description="Verify a release archive's completeness and checksum.")
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument("--platform", required=True, choices=PLATFORMS)
    args = parser.parse_args()

    defects = verify(args.archive, args.checksums, args.platform)
    if defects:
        for d in defects:
            print(f"FAIL: {d}", file=sys.stderr)
        return 1
    print(f"OK: {args.archive.name} complete and checksum-verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
