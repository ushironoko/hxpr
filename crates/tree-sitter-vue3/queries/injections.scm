; Vue 3 language injections
; Based on xiaoxin-sky/tree-sitter-vue3 with octorus customizations
;
; NOTE: For script elements, both the default JavaScript rule and lang-specific rules
; may match the same content. The Rust extract_injections function deduplicates these,
; preferring the more specific language (ts/tsx/jsx) over JavaScript.

; CSS in <style> (default)
((style_element
  (raw_text) @injection.content)
 (#set! injection.language "css"))

; JavaScript in <script> (default fallback)
; This matches all script elements; deduplication happens in Rust
((script_element
  (raw_text) @injection.content)
 (#set! injection.language "javascript"))

; TypeScript in <script lang="ts"> or <script lang="typescript">
((script_element
  (start_tag
    (attribute
      (attribute_name) @_attr
      (quoted_attribute_value (attribute_value) @_lang)))
  (raw_text) @injection.content)
 (#eq? @_attr "lang")
 (#match? @_lang "^(ts|typescript)$")
 (#set! injection.language "typescript"))

; TSX in <script lang="tsx"> (octorus customization)
((script_element
  (start_tag
    (attribute
      (attribute_name) @_attr
      (quoted_attribute_value (attribute_value) @_lang)))
  (raw_text) @injection.content)
 (#eq? @_attr "lang")
 (#eq? @_lang "tsx")
 (#set! injection.language "tsx"))

; JSX in <script lang="jsx"> (octorus customization)
((script_element
  (start_tag
    (attribute
      (attribute_name) @_attr
      (quoted_attribute_value (attribute_value) @_lang)))
  (raw_text) @injection.content)
 (#eq? @_attr "lang")
 (#eq? @_lang "jsx")
 (#set! injection.language "jsx"))

; JavaScript in interpolations {{ }}
((interpolation
  (raw_text) @injection.content)
 (#set! injection.language "javascript"))
