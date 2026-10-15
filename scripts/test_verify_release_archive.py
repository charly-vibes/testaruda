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
    return h.hexdigest()


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
        return path

    def tar_gz(self, basename, members, exec_bits=True, sha=None):
        """Create a tar.gz whose members have full exec bits by default."""
        path = self.tmp / basename
        with tarfile.open(path, "w:gz") as tar:
            for name in members:
                mode = 0o755 if exec_bits else 0o644
                info = tarfile.TarInfo(name)
                info.size = 4
                info.mode = mode
                tar.addfile(info, io.BytesIO(b"ELF\x00"))
        return path, self.checksums(basename, sha)

    def zip_exe(self, basename, members, exec_bits=True, sha=None):
        """Create a zip of Windows binaries; external_attr pins exec-ish DOS attr."""
        path = self.tmp / basename
        with zipfile.ZipFile(path, "w") as z:
            for name in members:
                zi = zipfile.ZipInfo(name)
                # Windows archives store no POSIX mode; keep a plain entry.
                zi.external_attr = (stat.S_IXUSR << 16) if exec_bits else 0
                z.writestr(zi, b"MZ\x00\x00")
        return path, self.checksums(basename, sha)


class TestCompleteArchivesAccepted(unittest.TestCase):
    """GREEN cases: complete, correct archives pass on every platform shape."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.fx = ArchiveFixture(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def test_complete_unix_archive_accepted(self):
        path, sums = self.fx.tar_gz(ARCHIVE_BASENAME, UNIX_MEMBERS)
        code, out = run_verifier(path, sums, UNIX_PLATFORM)
        self.assertEqual(code, 0, out)

    def test_complete_windows_archive_accepted(self):
        basename = f"testaruda_9.9.9_{WINDOWS_PLATFORM}.zip"
        path, sums = self.fx.zip_exe(basename, WINDOWS_MEMBERS)
        code, out = run_verifier(path, sums, WINDOWS_PLATFORM)
        self.assertEqual(code, 0, out)


class TestIncompleteArchivesRejected(unittest.TestCase):
    """RED cases: every completeness dimension must be enforced."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.fx = ArchiveFixture(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def test_missing_binary_rejected(self):
        short = UNIX_MEMBERS[:-1]
        path, sums = self.fx.tar_gz(ARCHIVE_BASENAME, short)
        code, out = run_verifier(path, sums, UNIX_PLATFORM)
        self.assertNotEqual(code, 0, "archive missing a binary must be rejected")
        self.assertIn("testaruda-adapter-clojure", out)

    def test_engine_only_staging_rejected(self):
        """Root-only staging (pre-#68 shape) must fail completeness."""
        path, sums = self.fx.tar_gz(ARCHIVE_BASENAME, ["testaruda"])
        code, out = run_verifier(path, sums, UNIX_PLATFORM)
        self.assertNotEqual(code, 0, out)

    def test_non_executable_unix_member_rejected(self):
        path, sums = self.fx.tar_gz(
            ARCHIVE_BASENAME, UNIX_MEMBERS, exec_bits=False)
        code, out = run_verifier(path, sums, UNIX_PLATFORM)
        self.assertNotEqual(code, 0, "644-mode members must be rejected")
        self.assertIn("exec", out.lower())

    def test_checksum_mismatch_rejected(self):
        path, sums = self.fx.tar_gz(ARCHIVE_BASENAME, UNIX_MEMBERS, sha=FAKE_SHA)
        code, out = run_verifier(path, sums, UNIX_PLATFORM)
        self.assertNotEqual(code, 0, out)

    def test_checksums_entry_missing_rejected(self):
        path, sums = self.fx.tar_gz(ARCHIVE_BASENAME, UNIX_MEMBERS)
        sums.write_text("")  # verifier's own archive absent from the manifest
        code, out = run_verifier(path, sums, UNIX_PLATFORM)
        self.assertNotEqual(code, 0, out)

    def test_unknown_platform_rejected(self):
        path, sums = self.fx.tar_gz(ARCHIVE_BASENAME, UNIX_MEMBERS)
        code, out = run_verifier(path, sums, "amiga_500")
        self.assertNotEqual(code, 0, out)


if __name__ == "__main__":
    unittest.main()
