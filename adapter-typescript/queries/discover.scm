; Test discovery query for TypeScript/TSX
; Captures describe, it, test blocks and parameterized .each variants

; describe/it/test call: describe("name", ...), it("name", ...), test("name", ...)
(call_expression
  function: (identifier) @_test_fn
  (#match? @_test_fn "^(describe|it|test)$")
  arguments: (arguments
    (string (string_fragment) @test_name))
  ) @test_declaration

; Parameterized test methods: describe.each(...)("name", ...), it.each(...)("name", ...), test.each(...)("name", ...)
(call_expression
  function: (call_expression
    function: (member_expression
      object: (identifier) @_each_obj
      property: (property_identifier) @_each_method
      (#match? @_each_method "^each$"))
    arguments: (_) @_data_table)
  arguments: (arguments
    (string (string_fragment) @test_name) _*)
  ) @test_declaration