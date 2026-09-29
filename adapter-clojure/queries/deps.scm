; deps.scm — Match :require, :use, and :import entries
;
; Two shapes:
;
; 1. Inside ns forms: (:require ...), (:use ...), (:import ...) — keyword head.
;    Captures:
;      @dep_form   — the full list
;      @dep_entry  — each dependency entry (vector or bare symbol)
;
;    Handles:
;      - (:require [namespace :as alias])       — vector notation
;      - (:require namespace)                   — bare symbol notation
;      - (:require [namespace :refer [foo]])    — vector with :refer
;      - (:require [namespace :refer :all])     — vector with :refer :all
;      - (:use [namespace :only [foo]])         — :use variant
;      - (:import java.util.Date)               — :import variant
;
; 2. Babashka-script style (gh-34/xcb6): top-level (require '[...]) —
;    symbol head with quoted entries. Captures the INNER vec/sym so the
;    extracted text has no leading quote char.
;
; Ignores via tree-sitter (no custom logic needed):
;   - comments, #_ discard forms, strings with parens, reader conditionals

; Shape 1: (:require ...) inside ns forms
(list_lit
  value: (kwd_lit) @_keyword
  (#match? @_keyword "^:(require|use|import)$")
  .
  ([(vec_lit) (sym_lit)] @dep_entry)+) @dep_form

; Shape 2: (require '[...]) / (require plain.ns) top-level forms
(list_lit
  value: (sym_lit) @_head_sym
  (#match? @_head_sym "^(require|use|import)$")
  .
  ([(quoting_lit (vec_lit) @dep_entry)
    (quoting_lit (sym_lit) @dep_entry)
    (vec_lit) @dep_entry
    (sym_lit) @dep_entry]+)) @dep_form
