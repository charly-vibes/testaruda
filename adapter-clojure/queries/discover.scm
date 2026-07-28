; discover.scm — Match deftest and deftest- forms for test discovery
;
; Captures:
;   @test_item — the full (deftest ...) or (deftest- ...) list expression
;   @test_name — the test name symbol (second child of the list)
;
; Handles:
;   - (deftest name ...)
;   - (deftest- name ...)
; Ignores via tree-sitter (no custom logic needed):
;   - comments, #_ discard forms, reader conditionals, strings with parens, metadata
(list_lit
  value: (sym_lit) @_deftest
  (#match? @_deftest "^(deftest|deftest-)$")
  .
  value: (sym_lit) @test_name) @test_item