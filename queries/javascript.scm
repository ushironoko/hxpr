; JavaScript/JSX highlight queries for tree-sitter
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

; Types (JSDoc)
(type_identifier) @type

; Functions
(function_declaration name: (identifier) @function)
(method_definition name: (property_identifier) @function.method)
(call_expression function: (identifier) @function.call)
(call_expression function: (member_expression property: (property_identifier) @function.method.call))
(arrow_function) @function

; Strings
(string) @string
(template_string) @string

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
(formal_parameters (identifier) @variable.parameter)

; Classes
(class_declaration name: (identifier) @type)

; Imports
(import_clause (identifier) @variable)
(named_imports (import_specifier name: (identifier) @variable))
