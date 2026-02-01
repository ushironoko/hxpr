; TypeScript/TSX highlight queries for tree-sitter
; Based on nvim-treesitter queries

; Keywords
[
  "as"
  "async"
  "await"
  "break"
  "case"
  "catch"
  "class"
  "const"
  "continue"
  "debugger"
  "default"
  "delete"
  "do"
  "else"
  "export"
  "extends"
  "finally"
  "for"
  "from"
  "function"
  "get"
  "if"
  "import"
  "in"
  "instanceof"
  "let"
  "new"
  "of"
  "return"
  "set"
  "static"
  "switch"
  "throw"
  "try"
  "typeof"
  "var"
  "void"
  "while"
  "with"
  "yield"
] @keyword

; TypeScript-specific keywords
[
  "abstract"
  "declare"
  "enum"
  "implements"
  "interface"
  "keyof"
  "namespace"
  "private"
  "protected"
  "public"
  "readonly"
  "type"
] @keyword

; Types
(type_identifier) @type
(predefined_type) @type.builtin

; Functions
(function_declaration name: (identifier) @function)
(method_definition name: (property_identifier) @function.method)
(call_expression function: (identifier) @function.call)
(call_expression function: (member_expression property: (property_identifier) @function.method.call))
(arrow_function) @function

; Strings
(string) @string
(template_string) @string
(template_literal_type) @string

; Numbers
(number) @number

; Booleans
(true) @boolean
(false) @boolean

; Null/undefined
(null) @constant.builtin
(undefined) @constant.builtin

; Comments
(comment) @comment

; Operators
[
  "!"
  "!="
  "!=="
  "%"
  "%="
  "&"
  "&&"
  "&&="
  "&="
  "*"
  "**"
  "**="
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
  "<<"
  "<<="
  "<="
  "="
  "=="
  "==="
  "=>"
  ">"
  ">="
  ">>"
  ">>="
  ">>>"
  ">>>="
  "?"
  "?."
  "??"
  "??="
  "^"
  "^="
  "|"
  "|="
  "||"
  "||="
  "~"
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
(property_identifier) @property
(shorthand_property_identifier) @property
(shorthand_property_identifier_pattern) @property

; this
(this) @variable.builtin

; Parameters
(required_parameter pattern: (identifier) @variable.parameter)
(optional_parameter pattern: (identifier) @variable.parameter)

; Classes
(class_declaration name: (type_identifier) @type)

; Interfaces
(interface_declaration name: (type_identifier) @type)

; Imports
(import_clause (identifier) @variable)
(named_imports (import_specifier name: (identifier) @variable))
