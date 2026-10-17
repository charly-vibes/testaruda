"""Purpose: Pin the release-quality contract that workspace crates are gated.

Responsibilities:
- Assert the root justfile quality recipes cover the whole cargo workspace
  (engine + shipped adapters), not just the root crate (testaruda-h7bv.1).
- Assert CI invokes the same justfile recipes so local and CI gates cannot
  drift apart.
- Assert release publication is coupled to workspace quality and archive
  verification on the tag commit (testaruda-h7bv.3): the publish job must
  need every build job plus a quality job, verify archives before creating
  the GitHub release, and host every publication path inside that gated job.

Rationale: Root-only `just ci` let sibling workspace adapters (adapter-*)
fail clippy/test/release builds while CI stayed green — the exact gap PR
quality gates exist to close. Publication previously depended on build jobs
only, so a red tag commit could still ship to GitHub/crates.io/tap/Scoop.
These contract tests fail fast if someone un-wires `--workspace` from the
recipes, stops CI from using them, or decouples publication from its gates.
"""

import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
JUSTFILE = (REPO / "justfile").read_text()
CI_WORKFLOW = (REPO / ".github/workflows/ci.yml").read_text()
RELEASE_WORKFLOW = (REPO / ".github/workflows/release.yml").read_text()

# Recipes that gate PR quality and release artifacts. Each must operate on
# the full workspace: the shipped adapters (adapter-*) are release binaries
# and must pass the same gates as the engine.
WORKSPACE_RECIPES = {
    "lint": r"clippy\s+--workspace\s+--all-targets",
    # DDL test-runner standard (DDL-85p): `test` is the testaruda exec loop
    # (its selection falls back to the full workspace suite), and the plain
    # workspace run stays wired as `test-full` — both required so the
    # shipped adapters keep full-suite coverage either way.
    "test": r"testaruda\s+exec",
    "test-full": r"cargo\s+test\s+--workspace",
    "build-release": r"cargo\s+build\s+--workspace\s+--release",
}


class ReleaseContract(unittest.TestCase):
    def test_workspace_quality_recipes_are_wired(self):
        for recipe, pattern in WORKSPACE_RECIPES.items():
            section = _recipe_body(recipe)
            self.assertRegex(
                section,
                pattern,
                f"justfile recipe `{recipe}` must run the workspace-wide command; "
                "root-only gating lets shipped adapters drift",
            )

    def test_ci_runs_the_justfile_gate(self):
        self.assertRegex(
            CI_WORKFLOW,
            r"run:\s*just\s+ci\b",
            "CI must invoke `just ci` (local/CI parity is the whole point)",
        )


def _yaml_job_block(workflow: str, job_name: str) -> str:
    """Return the raw YAML block of one top-level workflow job."""
    lines = workflow.splitlines()
    starts = [
        i
        for i, line in enumerate(lines)
        if re.match(rf"^  {re.escape(job_name)}:\s*(#.*)?$", line)
    ]
    assert starts, f"job `{job_name}` not found in workflow"
    start = starts[0] + 1
    end = len(lines)
    for i in range(start, len(lines)):
        # a new top-level job starts at exactly two-space indentation
        if lines[i] and not lines[i].startswith("    ") and lines[i].lstrip():
            end = i
            break
    return "\n".join(lines[start:end])


def _yaml_step_blocks(job_block: str) -> list[str]:
    """Split a job block into its `- name:`/`- uses:` step chunks."""
    chunks: list[str] = []
    current: list[str] = []
    for line in job_block.splitlines():
        if re.match(r"^\s+- (name|uses):", line) and current:
            chunks.append("\n".join(current))
            current = [line]
        else:
            current.append(line)
    if current:
        chunks.append("\n".join(current))
    return chunks


def _yaml_step_names(job_block: str) -> list[str]:
    """Ordered step names of a job block (uses-only steps get their action)."""
    names = []
    for chunk in _yaml_step_blocks(job_block):
        m = re.search(r"- name:\s*(.+)", chunk)
        if m:
            names.append(m.group(1).strip())
        else:
            m = re.search(r"- uses:\s*(\S+)", chunk)
            if m:
                names.append(m.group(1))
    return names


class PublicationGateContract(unittest.TestCase):
    """Publication must be gated on quality + archive verification (h7bv.3).

    Every publication path (GitHub release, crates.io, homebrew tap, scoop)
    lives inside the `publish` job, which needs every build job plus a
    workspace `quality` job — so a red tag commit cannot ship anywhere.
    """

    BUILD_JOBS = ["build-linux", "build-darwin", "build-windows"]
    PUBLICATION_STEP_MARKERS = [
        "gh release create",
        "cargo publish",
        "scripts/update-homebrew.py",
        "scripts/update-scoop.py",
    ]

    def setUp(self):
        self.publish = _yaml_job_block(RELEASE_WORKFLOW, "publish")

    def test_publish_needs_every_build_job_and_quality(self):
        needs = re.search(r"^\s+needs:\s*\[(.*)\]", self.publish, re.M)
        self.assertIsNotNone(needs, "publish job must declare its needs")
        needed = [n.strip() for n in needs.group(1).split(",")]
        for job in self.BUILD_JOBS + ["quality"]:
            self.assertIn(
                job,
                needed,
                f"publish must need `{job}` — publication without it is ungated",
            )

    def test_quality_job_runs_workspace_quality_recipes(self):
        quality = _yaml_job_block(RELEASE_WORKFLOW, "quality")
        self.assertRegex(
            quality,
            r"just\s+lint\b",
            "quality job must run the workspace clippy gate",
        )
        self.assertRegex(
            quality,
            r"just\s+test\b",
            "quality job must run the workspace test gate",
        )
        self.assertRegex(
            quality,
            r"just\s+fmt-check\b",
            "quality job must run the format gate",
        )

    def test_archive_verification_precedes_release_creation(self):
        names = _yaml_step_names(self.publish)
        verify_idx = next(
            (i for i, n in enumerate(names) if "Verify archive completeness" in n),
            None,
        )
        release_idx = next(
            (i for i, n in enumerate(names) if "Create GitHub release" in n), None
        )
        self.assertIsNotNone(verify_idx, "archive verification step missing")
        self.assertIsNotNone(release_idx, "release creation step missing")
        self.assertLess(
            verify_idx,
            release_idx,
            "archives must be verified BEFORE the GitHub release is created",
        )

    def test_every_publication_path_lives_in_the_gated_publish_job(self):
        for marker in self.PUBLICATION_STEP_MARKERS:
            self.assertIn(
                marker,
                self.publish,
                f"publication path `{marker}` escaped the gated publish job",
            )


class PublicationGateSimulation(unittest.TestCase):
    """Local job-graph simulation of GitHub Actions `needs` semantics.

    Disposable stand-in for a live dry run: models how a publish job with
    plain (unconditional) needs concludes for success, failure, skipped
    and cancelled upstream states. The gate holds iff publish runs only
    when every need succeeded.
    """

    NEEDS = ["build-linux", "build-darwin", "build-windows", "quality"]

    @staticmethod
    def conclude_publish(upstream):
        """GitHub Actions default semantics for an unconditional-needs job.

        publish runs iff every need concluded success. Any failure/skipped/
        cancelled need makes publish skipped; a cancelled run cancels it.
        """
        if "cancelled" in upstream.values():
            return "cancelled"
        if any(s != "success" for s in upstream.values()):
            return "skipped"
        return "success"

    def assert_publish_not_published(self, upstream):
        self.assertIn(
            self.conclude_publish(upstream), ("skipped", "cancelled")
        )

    def test_all_green_publish_runs(self):
        upstream = {job: "success" for job in self.NEEDS}
        self.assertEqual(self.conclude_publish(upstream), "success")

    def test_failed_quality_job_skips_publication(self):
        upstream = {job: "success" for job in self.NEEDS}
        upstream["quality"] = "failure"
        self.assert_publish_not_published(upstream)

    def test_failed_build_job_skips_publication(self):
        upstream = {job: "success" for job in self.NEEDS}
        upstream["build-windows"] = "failure"
        self.assert_publish_not_published(upstream)

    def test_skipped_quality_job_skips_publication(self):
        upstream = {job: "success" for job in self.NEEDS}
        upstream["quality"] = "skipped"
        self.assert_publish_not_published(upstream)

    def test_cancelled_run_cancels_publication(self):
        upstream = {job: "success" for job in self.NEEDS}
        upstream["build-darwin"] = "cancelled"
        self.assert_publish_not_published(upstream)


def _recipe_body(name: str) -> str:
    """Return the body of a justfile recipe, resolving alias recipes."""
    lines = JUSTFILE.splitlines()
    for _ in range(4):  # alias chains are shallow; bail out rather than loop
        body: list[str] = []
        deps: list[str] = []
        in_recipe = False
        for line in lines:
            if re.match(rf"^{re.escape(name)}:\s*(#.*)?$", line) or re.match(
                rf"^{re.escape(name)}:\s+(\S.*)?$", line
            ):
                in_recipe = True
                deps = re.split(r"\s+", line.split(":", 1)[1].strip()) if ":" in line else []
                deps = [d for d in deps if d]
                continue
            if in_recipe:
                if line and not line[0].isspace():
                    break  # next recipe (or comment block) started
                body.append(line)
        if any(line.strip() for line in body):
            return "\n".join(body)
        assert deps, f"recipe `{name}` not found or empty in justfile"
        name = deps[0]
    raise AssertionError(f"alias chain for recipe too deep at `{name}`")
