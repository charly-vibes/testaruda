"""Multi-language adapter import-parsing validator (package).

Split out of scripts/validate-imports.py to stay under the 300-line
script gate; the dash-named script remains the executable shim.
"""

from validate_imports.base import LanguageBackend
from validate_imports.python_backend import PythonBackend
from validate_imports.rust_backend import RustBackend
from validate_imports.julia_backend import JuliaBackend
from validate_imports.ts_backend import TypeScriptBackend
from validate_imports.clj_backend import ClojureBackend
from validate_imports.python_backend import _py_adapter_imports, _py_ast_imports

BACKENDS: dict[str, type[LanguageBackend]] = {
    "python": PythonBackend,
    "rust": RustBackend,
    "julia": JuliaBackend,
    "typescript": TypeScriptBackend,
    "clojure": ClojureBackend,
}

ADAPTER_MAP: dict[str, str] = {
    "python": "testaruda-adapter-python",
    "rust": "testaruda-adapter-rust",
    "julia": "testaruda-adapter-julia",
    "typescript": "testaruda-adapter-typescript",
    "clojure": "testaruda-adapter-clojure",
}

__all__ = [
    "LanguageBackend", "BACKENDS", "ADAPTER_MAP", "_py_adapter_imports", "_py_ast_imports",
    "PythonBackend", "RustBackend", "JuliaBackend", "TypeScriptBackend",
    "ClojureBackend",
]
