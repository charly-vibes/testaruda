"""Julia backend: import/using parsing (both sides use the same parser)."""

import os

from validate_imports.base import LanguageBackend

_JULIA_BUILTINS = ("Base", "Core", "Main", "Test")


class JuliaBackend(LanguageBackend):

    @property
    def name(self) -> str: return "julia"

    @property
    def source_extensions(self) -> tuple[str, ...]: return (".jl",)

    def is_test_file(self, rel_path: str) -> bool:
        return "/test/" in rel_path or rel_path.startswith("test/")

    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _julia_parse_imports(content)

    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        """The Julia adapter uses ReTestItems. Import parsing is done by
        Testimonial.jl internally. We replicate a reasonable parser."""
        return _julia_parse_imports(content)


def _julia_deps_from_statement(rest: str) -> set[str]:
    """Deps from the comma-separated tail of a using/import statement."""
    deps: set[str] = set()
    for part in rest.split(","):
        part = part.strip()
        # Handle: import Foo.Bar: baz
        if ":" in part:
            part = part.split(":")[0].strip()
        # Handle: using .ModuleName (relative), using Foo: bar
        m = part.lstrip(".").split(".")[0].strip()
        if m:
            deps.add(m)
    return deps


def _julia_parse_imports(content: str) -> set[str]:
    """Parse Julia import/using statements.

    @testitem/@test/@eval/@generated don't import; only using/import count.
    """
    deps: set[str] = set()
    for line in content.split("\n"):
        t = line.strip()
        if t.startswith("using ") or t.startswith("import "):
            rest = t[6:] if t.startswith("using ") else t[7:]
            deps |= _julia_deps_from_statement(rest)
    # Filter out common Julia builtins
    return {d for d in deps if d not in _JULIA_BUILTINS}
