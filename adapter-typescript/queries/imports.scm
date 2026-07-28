; Import extraction query for TypeScript/TSX
; Captures ES module imports, require() calls, and dynamic import() expressions

; ES module imports: import ... from "module"
; CST: import_statement → import, import_clause, from, string, ;
(import_statement
  (string (string_fragment) @import_source))

; require() calls: const x = require("module")
; CST: call_expression → identifier "require", arguments → ( string )
(call_expression
  function: (identifier) @_require_fn
  (#eq? @_require_fn "require")
  arguments: (arguments (string (string_fragment) @require_source)))

; Dynamic import(): import("module")
; CST: call_expression → import keyword, arguments → ( string )
(call_expression
  function: (import)
  arguments: (arguments (string (string_fragment) @dynamic_import_source)))
