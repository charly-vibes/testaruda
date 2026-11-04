"""TypeScript backend: import/require parsing (both sides use the same parser)."""

import os
import re

from validate_imports.base import LanguageBackend

_TS_FROM_RE = re.compile(
    r"^import\s+(?:\{[^}]*\}|\*\s+as\s+\w+|\w+(?:\s*,\s*\{[^}]*\})?)\s+from\s+['\"]([^'\"]+)['\"]"
)
_TS_SIDE_EFFECT_RE = re.compile(r"^import\s+['\"]([^'\"]+)['\"]")
_TS_REQUIRE_RE = re.compile(r"require\(['\"]([^'\"]+)['\"]\)")


class TypeScriptBackend(LanguageBackend):

    @property
    def name(self) -> str: return "typescript"

    @property
    def source_extensions(self) -> tuple[str, ...]: return (".ts", ".tsx", ".js", ".jsx")

    def is_test_file(self, rel_path: str) -> bool:
        base = os.path.basename(rel_path)
        return (
            base.startswith("test_")
            or base.endswith(".test.ts")
            or base.endswith(".spec.ts")
            or "/__tests__/" in rel_path
            or "/test/" in rel_path
        )

    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _ts_parse_imports(content)

    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """Replicate what the TypeScript adapter would do (tree-sitter based)."""
        return _ts_parse_imports(content)


def _ts_module(dep: str) -> str | None:
    """External module name (relative/absolute paths are not deps)."""
    if dep.startswith(".") or dep.startswith("/"):
        return None
    return dep.split("/")[0]


def _ts_parse_line(t: str) -> str | None:
    """Module referenced by one import-ish line, if any."""
    m = _TS_FROM_RE.match(t)
    if m:
        return m.group(1)
    # import 'module' (side-effect import)
    m = _TS_SIDE_EFFECT_RE.match(t)
    if m:
        return m.group(1)
    # require('module')
    m = _TS_REQUIRE_RE.search(t)
    if m:
        return m.group(1)
    return None


def _ts_parse_imports(content: str) -> set[str]:
    """Parse TypeScript import statements."""
    deps: set[str] = set()
    for line in content.split("\n"):
        mod = _ts_parse_line(line.strip())
        if mod is not None:
            name = _ts_module(mod)
            if name:
                deps.add(name)
    return deps
