; Python highlight queries for tree-sitter
; Based on nvim-treesitter queries

; Keywords
[
  "and"
  "as"
  "assert"
  "async"
  "await"
  "break"
  "class"
  "continue"
  "def"
  "del"
  "elif"
  "else"
  "except"
  "exec"
  "finally"
  "for"
  "from"
  "global"
  "if"
  "import"
  "in"
  "is"
  "lambda"
  "nonlocal"
  "not"
  "or"
  "pass"
  "print"
  "raise"
  "return"
  "try"
  "while"
  "with"
  "yield"
  "match"
  "case"
] @keyword

; Types
(type) @type

; Builtin types
((identifier) @type.builtin
  (#any-of? @type.builtin
    "bool"
    "bytes"
    "complex"
    "dict"
    "float"
    "frozenset"
    "int"
    "list"
    "object"
    "set"
    "str"
    "tuple"
    "type"))

; Functions
(function_definition name: (identifier) @function)
(call function: (identifier) @function.call)
(call function: (attribute attribute: (identifier) @function.method.call))

; Builtin functions
((identifier) @function.builtin
  (#any-of? @function.builtin
    "abs"
    "all"
    "any"
    "ascii"
    "bin"
    "bool"
    "breakpoint"
    "bytearray"
    "bytes"
    "callable"
    "chr"
    "classmethod"
    "compile"
    "complex"
    "delattr"
    "dict"
    "dir"
    "divmod"
    "enumerate"
    "eval"
    "exec"
    "filter"
    "float"
    "format"
    "frozenset"
    "getattr"
    "globals"
    "hasattr"
    "hash"
    "help"
    "hex"
    "id"
    "input"
    "int"
    "isinstance"
    "issubclass"
    "iter"
    "len"
    "list"
    "locals"
    "map"
    "max"
    "memoryview"
    "min"
    "next"
    "object"
    "oct"
    "open"
    "ord"
    "pow"
    "print"
    "property"
    "range"
    "repr"
    "reversed"
    "round"
    "set"
    "setattr"
    "slice"
    "sorted"
    "staticmethod"
    "str"
    "sum"
    "super"
    "tuple"
    "type"
    "vars"
    "zip"))

; Strings
(string) @string
(interpolation) @string.special

; Numbers
(integer) @number
(float) @number.float

; Booleans
(true) @boolean
(false) @boolean

; None
(none) @constant.builtin

; Comments
(comment) @comment

; Operators
[
  "+"
  "-"
  "*"
  "/"
  "%"
  "**"
  "//"
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "="
  "+="
  "-="
  "*="
  "/="
  "//="
  "%="
  "**="
  "&="
  "|="
  "^="
  "<<="
  ">>="
  "~"
  "&"
  "|"
  "^"
  "<<"
  ">>"
  "@"
  "@="
  "->"
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
(attribute attribute: (identifier) @property)

; Parameters
(parameters (identifier) @variable.parameter)
(default_parameter name: (identifier) @variable.parameter)
(typed_parameter (identifier) @variable.parameter)
(typed_default_parameter name: (identifier) @variable.parameter)
(keyword_argument name: (identifier) @variable.parameter)

; Decorators
(decorator "@" @attribute)
(decorator (identifier) @attribute)
(decorator (call function: (identifier) @attribute))

; self
((identifier) @variable.builtin
  (#eq? @variable.builtin "self"))

; cls
((identifier) @variable.builtin
  (#eq? @variable.builtin "cls"))

; Classes
(class_definition name: (identifier) @type)

; Imports
(import_statement (dotted_name (identifier) @namespace))
(import_from_statement module_name: (dotted_name (identifier) @namespace))
(aliased_import name: (dotted_name (identifier) @namespace))
