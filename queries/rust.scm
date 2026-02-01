; Rust highlight queries for tree-sitter
; Minimal version for tree-sitter-rust 0.24

; Types
(type_identifier) @type
(primitive_type) @type.builtin

; Functions
(function_item name: (identifier) @function)
(call_expression function: (identifier) @function.call)
(macro_invocation macro: (identifier) @function.macro)

; Strings
(string_literal) @string
(raw_string_literal) @string
(char_literal) @character

; Numbers
(integer_literal) @number
(float_literal) @number.float

; Booleans
(boolean_literal) @boolean

; Comments
(line_comment) @comment
(block_comment) @comment

; Punctuation brackets
(token_tree [ "(" ")" "[" "]" "{" "}" ] @punctuation.bracket)
(parameters [ "(" ")" ] @punctuation.bracket)
(type_parameters [ "<" ">" ] @punctuation.bracket)
(block [ "{" "}" ] @punctuation.bracket)

; Identifiers
(identifier) @variable
(field_identifier) @property
(shorthand_field_identifier) @property

; Attributes
(attribute_item) @attribute
(inner_attribute_item) @attribute

; Lifetimes
(lifetime) @label

; Parameters
(parameter pattern: (identifier) @variable.parameter)

; Modules
(mod_item name: (identifier) @namespace)

; Use declarations
(use_declaration argument: (identifier) @namespace)
(use_declaration argument: (scoped_identifier) @namespace)
