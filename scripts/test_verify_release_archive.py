"""Purpose: RED/GREEN meter for the release-archive completeness verifier.

Responsibilities:
- Pin the contract of scripts/verify-release-archive.py: a release archive
  must contain every advertised binary, carry the executable bit on Unix,
  match its checksums.txt entry, and be rejected otherwise (testaruda-h7bv.2).
- Prove incomplete archives are rejected with synthetic fixtures (missing
  binary, missing exec bit, checksum mismatch, checksums entry absent,
  unknown platform) and complete Unix/Windows archives are accepted.

Rationale: PR #68 grew the archives to all five advertised binaries but
nothing verifies the artifact after packaging — a regression ships silently
(exact failure mode of the DDL-3av exec-bit incident). These tests fail
fast if the verifier stops checking any completeness dimension.
"""

import hashlib
import io
import stat
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "verify-release-archive.py"

UNIX_PLATFORM = "linux_amd64"
WINDOWS_PLATFORM = "windows_amd64"
ARCHIVE_BASENAME = f"testaruda_9.9.9_{UNIX_PLATFORM}.tar.gz"

# All five advertised binaries — README Adapters table is the source of truth.
UNIX_MEMBERS = [
    "testaruda",
    "testaruda-adapter-rust",
    "testaruda-adapter-python",
    "testaruda-adapter-typescript",
    "testaruda-adapter-clojure",
]
WINDOWS_MEMBERS = [name + ".exe" for name in UNIX_MEMBERS]

FAKE_SHA = "0" * 64


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    digest = h.hexdigest()
    assert len(digest) == 64, "sha256 hex digest must be 64 chars"
    return digest


def run_verifier(archive, checksums, platform):
    """Run the verifier as a subprocess; return (returncode, stderr_tail)."""
    import subprocess

    proc = subprocess.run(
        ["python3", str(SCRIPT),
         "--archive", str(archive),
         "--checksums", str(checksums),
         "--platform", platform],
        capture_output=True,
        text=True,
        cwd=str(REPO),
    )
    assert archive.exists(), "archive under test must exist before verification"
    return proc.returncode, proc.stderr + proc.stdout


class ArchiveFixture:
    """Build synthetic archives + checksums files in a temp dir."""

    def __init__(self, tmp):
        self.tmp = Path(tmp)

    def checksums(self, basename, sha=None):
        path = self.tmp / "checksums.txt"
        if sha is None:
            sha = sha256_of(self.tmp / basename)
        path.write_text(f"{sha}  {basename}\n")
        assert f"{sha}  {basename}" in path.read_text(), "manifest entry written"
        return path

    def tar_gz(self, basename, members, exec_bits=True):
        """Create a tar.gz whose members have full exec bits by default."""
        path = self.tmp / basename
        with tarfile.open(path, "w:gz") as tar:
            for name in members:
                mode = 0o755 if exec_bits else 0o644
                info = tarfile.TarInfo(name)
                info.size = 4
                info.mode = mode
                tar.addfile(info, io.BytesIO(b"ELF\x00"))
        assert path.exists(), "archive file must exist after building"
        with tarfile.open(path, "r:gz") as tar:
            assert tar.getnames() == list(members), "archive members written verbatim"
        return path, self.checksums(basename)

    def zip_exe(self, basename, members, exec_bits=True):
        """Create a zip of Windows binaries; external_attr pins exec-ish DOS attr."""
        path = self.tmp / basename
        with zipfile.ZipFile(path, "w") as z:
            for name in members:
                zi = zipfile.ZipInfo(name)
                # Windows archives store no POSIX mode; keep a plain entry.
                zi.external_attr = (stat.S_IXUSR << 16) if exec_bits else 0
                z.writestr(zi, b"MZ\x00\x00")
        assert path.exists(), "archive file must exist after building"
        with zipfile.ZipFile(path) as z:
            assert z.namelist() == list(members), "archive members written verbatim"
        return path, self.checksums(basename)


class TestArchiveVerifierContract(unittest.TestCase):
    """Fixture construction is inlined (no setUp): each subTest builds a
    synthetic archive and pins the verifier's accept/reject decision."""

    def test_complete_archives_accepted(self):
        """GREEN cases: complete, correct archives pass on every platform."""
        cases = [
            (
                "unix tar.gz with all members, exec bits",
                lambda fx: fx.tar_gz(ARCHIVE_BASENAME, UNIX_MEMBERS),
                UNIX_PLATFORM,
            ),
            (
                "windows zip with all .exe members",
                lambda fx: fx.zip_exe(
                    f"testaruda_9.9.9_{WINDOWS_PLATFORM}.zip", WINDOWS_MEMBERS
                ),
                WINDOWS_PLATFORM,
            ),
        ]
        self.assertEqual(len(cases), 2, "both platform shapes must be covered")
        for desc, build, platform in cases:
            with self.subTest(desc):
                with tempfile.TemporaryDirectory() as tmp:
                    path, sums = build(ArchiveFixture(tmp))
                    code, out = run_verifier(path, sums, platform)
                    self.assertEqual(code, 0, out)

    def test_incomplete_archives_rejected(self):
        """RED cases: every completeness dimension must be enforced."""
        short = [name for name in UNIX_MEMBERS if "adapter" not in name]
        cases = [
            (
                "archive missing a binary",
                lambda fx: fx.tar_gz(ARCHIVE_BASENAME, short),
                UNIX_PLATFORM,
                "testaruda-adapter-clojure",
            ),
            (
                "root-only staging (pre-#68 shape) fails completeness",
                lambda fx: fx.tar_gz(ARCHIVE_BASENAME, ["testaruda"]),
                UNIX_PLATFORM,
                "",
            ),
            (
                "644-mode members rejected",
                lambda fx: fx.tar_gz(ARCHIVE_BASENAME, UNIX_MEMBERS, exec_bits=False),
                UNIX_PLATFORM,
                "exec",
            ),
            (
                "checksum mismatch rejected",
                lambda fx: _with_fake_sha(fx.tar_gz(ARCHIVE_BASENAME, UNIX_MEMBERS), FAKE_SHA),
                UNIX_PLATFORM,
                "",
            ),
            (
                "checksums entry missing rejected",
                lambda fx: _with_empty_manifest(fx.tar_gz(ARCHIVE_BASENAME, UNIX_MEMBERS)),
                UNIX_PLATFORM,
                "",
            ),
            (
                "unknown platform rejected",
                lambda fx: fx.tar_gz(ARCHIVE_BASENAME, UNIX_MEMBERS),
                "amiga_500",
                "",
            ),
        ]
        self.assertEqual(len(cases), 6, "all rejection dimensions must be covered")
        for desc, build, platform, expected in cases:
            with self.subTest(desc):
                with tempfile.TemporaryDirectory() as tmp:
                    path, sums = build(ArchiveFixture(tmp))
                    code, out = run_verifier(path, sums, platform)
                    self.assertNotEqual(code, 0, out)
                    if expected == "exec":
                        self.assertIn("exec", out.lower())
                    elif expected:
                        self.assertIn(expected, out)


def _with_fake_sha(built, sha):
    """Overwrite the manifest with a wrong digest for the archive."""
    path, sums = built
    basename = path.name
    sums.write_text(f"{sha}  {basename}\n")
    return path, sums


def _with_empty_manifest(built):
    """Empty the checksums manifest — verifier's own archive absent."""
    path, sums = built
    sums.write_text("")
    return path, sums
