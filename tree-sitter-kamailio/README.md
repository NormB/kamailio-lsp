# tree-sitter-kamailio

Tree-sitter grammar for the Kamailio configuration file language
(`kamailio.cfg`) — error-tolerant parsing for editor highlighting,
folding, and structural navigation.

Coverage (v0): comments, strings, numbers, preprocessor directives
(`#!define`, `#!ifdef`/`#!endif`, `#!substdef`, ...), `include_file`,
global assignments, `loadmodule`/`modparam`, every route-family block
(`request_route`, `route[NAME]`, `event_route[mod:event]`, ...),
`if`/`else`/`while`/`switch`, calls, operators, pseudo-variables and
transformations.

```sh
tree-sitter generate   # produces src/ (not committed)
tree-sitter test       # corpus tests in test/corpus/
```
