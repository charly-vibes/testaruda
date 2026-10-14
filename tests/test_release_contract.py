"""Purpose: Pin the release-quality contract that workspace crates are gated.

Responsibilities:
- Assert the root justfile quality recipes cover the whole cargo workspace
  (engine + shipped adapters), not just the root crate (testaruda-h7bv.1).
- Assert CI invokes the same justfile recipes so local and CI gates cannot
  drift apart.

Rationale: Root-only `just ci` let sibling workspace adapters (adapter-*)
fail clippy/test/release builds while CI stayed green — the exact gap PR
quality gates exist to close. These contract tests fail fast if someone
un-wires `--workspace` from the recipes or stops CI from using them.
"""

import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
JUSTFILE = (REPO / "justfile").read_text()
CI_WORKFLOW = (REPO / ".github/workflows/ci.yml").read_text()

# Recipes that gate PR quality and release artifacts. Each must operate on
# the full workspace: the shipped adapters (adapter-*) are release binaries
# and must pass the same gates as the engine.
WORKSPACE_RECIPES = {
    "lint": r"clippy\s+--workspace\s+--all-targets",
    "test": r"cargo\s+test\s+--workspace",
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
