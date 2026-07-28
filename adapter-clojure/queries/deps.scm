; deps.scm — Match :require, :use, and :import entries inside ns forms
;
; Captures:
;   @dep_form   — the full (:require ...), (:use ...), or (:import ...) list
;   @dep_entry  — each dependency entry (vector or bare symbol)
;
; Handles:
;   - (:require [namespace :as alias])       — vector notation
;   - (:require namespace)                   — bare symbol notation
;   - (:require [namespace :refer [foo]])    — vector with :refer
;   - (:require [namespace :refer :all])     — vector with :refer :all
;   - (:use [namespace :only [foo]])         — :use variant
;   - (:import java.util.Date)               — :import variant
; Ignores via tree-sitter (no custom logic needed):
;   - comments, #_ discard forms, strings with parens, reader conditionals
(list_lit
  value: (kwd_lit) @_keyword
  (#match? @_keyword "^:(require|use|import)$")
  .
  [(vec_lit) (sym_lit)] @dep_entry) @dep_form