; Go highlight queries for tree-sitter
; Based on nvim-treesitter queries

; Keywords
[
  "break"
  "case"
  "chan"
  "const"
  "continue"
  "default"
  "defer"
  "else"
  "fallthrough"
  "for"
  "func"
  "go"
  "goto"
  "if"
  "import"
  "interface"
  "map"
  "package"
  "range"
  "return"
  "select"
  "struct"
  "switch"
  "type"
  "var"
] @keyword

; Types
(type_identifier) @type
(type_spec name: (type_identifier) @type)

; Builtin types
((type_identifier) @type.builtin
  (#any-of? @type.builtin
    "bool"
    "byte"
    "complex128"
    "complex64"
    "error"
    "float32"
    "float64"
    "int"
    "int16"
    "int32"
    "int64"
    "int8"
    "rune"
    "string"
    "uint"
    "uint16"
    "uint32"
    "uint64"
    "uint8"
    "uintptr"))

; Functions
(function_declaration name: (identifier) @function)
(method_declaration name: (field_identifier) @function.method)
(call_expression function: (identifier) @function.call)
(call_expression function: (selector_expression field: (field_identifier) @function.method.call))

; Builtin functions
((identifier) @function.builtin
  (#any-of? @function.builtin
    "append"
    "cap"
    "clear"
    "close"
    "complex"
    "copy"
    "delete"
    "imag"
    "len"
    "make"
    "max"
    "min"
    "new"
    "panic"
    "print"
    "println"
    "real"
    "recover"))

; Strings
(raw_string_literal) @string
(interpreted_string_literal) @string
(rune_literal) @character

; Numbers
(int_literal) @number
(float_literal) @number.float
(imaginary_literal) @number

; Booleans
(true) @boolean
(false) @boolean

; Nil
(nil) @constant.builtin

; Comments
(comment) @comment

; Operators
[
  "!"
  "!="
  "%"
  "%="
  "&"
  "&&"
  "&="
  "&^"
  "&^="
  "*"
  "*="
  "+"
  "++"
  "+="
  "-"
  "--"
  "-="
  "/"
  "/="
  "<"
  "<-"
  "<<"
  "<<="
  "<="
  "="
  "=="
  ">"
  ">="
  ">>"
  ">>="
  "^"
  "^="
  "|"
  "|="
  "||"
  ":="
] @operator

; Punctuation
[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  "."
  ":"
  ";"
] @punctuation.delimiter

; Identifiers
(identifier) @variable
(field_identifier) @property

; Parameters
(parameter_declaration name: (identifier) @variable.parameter)

; Packages
(package_identifier) @namespace
(package_clause (package_identifier) @namespace)
(import_spec path: (interpreted_string_literal) @string)
