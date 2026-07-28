; ns.scm — Match (ns ...) declarations and extract the namespace name
;
; Captures:
;   @ns_form       — the full (ns ...) list expression
;   @namespace_name — the namespace symbol (second child, after any metadata)
;
; Handles:
;   - (ns name ...)                                 — simple
;   - (ns ^{:doc "..."} name ...)                   — with metadata
;   - (ns ^:deprecated name ...)                    — with metadata shortcut
; Ignores via tree-sitter (no custom logic needed):
;   - ns inside strings, ns as keyword, nested ns forms
(list_lit
  value: (sym_lit) @_ns
  (#eq? @_ns "ns")
  .
  value: (sym_lit) @namespace_name) @ns_form