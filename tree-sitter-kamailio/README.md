# tree-sitter-kamailio

Tree-sitter grammar for the Kamailio configuration file language
(`kamailio.cfg`) — error-tolerant parsing for editor highlighting,
folding, and structural navigation.

Coverage: comments (block comments nest, as in Kamailio), strings
(multi-line, and adjacent strings folded into one), numbers, IPv4 and
IPv6 addresses and networks, preprocessor directives (`#!define`,
`#!ifdef`/`#!endif`, `#!substdef`, ...) wherever they stand,
`include_file` (also inside route blocks), global assignments,
`listen` with `advertise`/`name`/`virtual`, `socket = { ... }` blocks,
`flags`/`avpflags` declarations, custom variables
(`group.name = value desc "..."`), `loadmodule`/`loadmodulex`,
`modparam`/`modparamx`, `cfgengine`, every route-family block
(`request_route`, `route[NAME]`, `event_route[mod:event]`, ...),
`if`/`else`/`while`/`switch`, calls, the full operator set of `cfg.y`
(including `eq`/`ne`/`ieq`/`ine`, `defined`, `(int)`/`(str)` casts),
selects (`@via[1].host`), `msg:len`, pseudo-variables and
transformations.

The kamailio-lsp test suite parses every `.cfg` that ships in
the pinned Kamailio tree and that `kamailio -c` does not reject as a
syntax error, plus the fixtures in `tests/grammar/`, and fails on any
parse error. It first blanks the lines the preprocessor skips, judged
from the config's own defines, because text in a switched-off
`#!ifdef` is not Kamailio code.

One limit follows from parsing text, not preprocessed text: both
branches of an `#!ifdef`/`#!else` are read as if both were there, so a
construct split across the branches — a `}` in each — shows as an
error.

## Files

- `src/parser.c` — generated from `grammar.js`, committed, and checked
  against a fresh `tree-sitter generate` by the test suite.
- `src/scanner.c` — hand-written; matches nested block comments, which
  no regular expression can. Build it together with `src/parser.c`
  (Neovim: `files = { "src/parser.c", "src/scanner.c" }`; Helix and Zed
  pick it up on their own).
- `queries/highlights.scm` and `queries/folds.scm` — highlighting and
  folding, using capture names Neovim, Helix and Zed all understand.

```sh
npx tree-sitter-cli@0.26 generate   # regenerates src/parser.c
npx tree-sitter-cli@0.26 test       # corpus tests in test/corpus/ and
                                    # highlight tests in test/highlight/
```
