# tree-sitter-vue3

Vue 3 grammar for [tree-sitter](https://github.com/tree-sitter/tree-sitter).

This is a local path dependency for [octorus](https://github.com/ushironoko/octorus), based on [xiaoxin-sky/tree-sitter-vue3](https://github.com/xiaoxin-sky/tree-sitter-vue3).

## Why a local dependency?

The upstream `tree-sitter-vue3` crate depends on `tree-sitter ~0.20.3`, which is incompatible with octorus's `tree-sitter 0.26.3`. Additionally, the upstream is marked as WIP and hasn't been updated since September 2022.

This local crate:
- Updates the tree-sitter dependency to 0.26.x
- Adds TSX/JSX injection support
- Maintains compatibility with octorus's tree-sitter version

## License

MIT (same as upstream)

## Upstream

Grammar and parser files from https://github.com/xiaoxin-sky/tree-sitter-vue3
