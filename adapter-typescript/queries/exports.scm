; Export extraction query for TypeScript/TSX
; Captures named exports, default exports, export lists, and re-exports

; Exported const: export const foo = ...
(export_statement
  (lexical_declaration
    (variable_declarator (identifier) @export_name))) @export_decl

; Exported function: export function foo() {}
(export_statement
  (function_declaration (identifier) @export_name)) @export_decl

; Exported class: export class Foo {}
; Note: class name is a type_identifier, not identifier
(export_statement
  (class_declaration (type_identifier) @export_name)) @export_decl

; Export list: export { foo, bar }
(export_statement
  (export_clause
    (export_specifier (identifier) @export_name))) @export_clause

; Default export: export default function() {} or export default class {}
(export_statement
  (function_expression)) @export_default

; Re-export: export * from "./module" or export * as X from "./module"
; Both have a string child with the source path
(export_statement
  (string (string_fragment) @re_export_source)) @re_export_stmt

; Namespace re-export: export * as Utils from "./module"
(export_statement
  (namespace_export (identifier) @re_export_name)) @re_export_stmt_ns