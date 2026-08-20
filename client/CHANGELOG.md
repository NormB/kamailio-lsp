# Changelog

All notable changes to the Kamailio Routing Script extension.

## [0.1.2] — 2026-08-20

- **Rename is parser-safe**: new names are gated on the charset
  Kamailio accepts for unquoted route names (`[A-Za-z_][A-Za-z0-9_]*`
  — dotted/dashed/colon names only parse quoted), and event-route
  names (`event_route[mod:event]`) are excluded from rename entirely
  (the module defines the event); references/highlights on them keep
  working.
- **Errors inside included files now surface**: Kamailio reports them
  against the include path as written (often relative), which used to
  be filtered out — a broken include showed a clean editor. They now
  attach to the `include_file` directive with the real file and line
  in the message, with a root-level fallback when nothing else could
  be attached.
- **Preprocessor-aware analysis**: `#!`/`!!` directives (indented
  too) are no longer treated as comments — multi-line `#!define`
  bodies can't leak phantom routes/modules, `#!include_file` and
  `!!import_file` are followed, and `//` line comments are
  recognized.
- **Six more modules documented**: READMEs titled `Exported
  parameters` (nat_traversal, call_control) and nested chapters
  (kafka, keepalive, lrkproxy, seas) now harvest.
- **All load forms recognized**: `loadmodule("x.so")`,
  `loadmodule("x", "opts")`, `loadmodulex`, `modparamx`; `xlog` is no
  longer offered as a core keyword (it is a module function).
- **Grammars match the lexer**: tree-sitter drops `!~`/`%`/`+=`/`-=`/
  `break(arg)`/named `request_route`, gains `and`/`or`/`not`/`mod`/
  `|`/`&`, paren-less `return`, `!!` directives, `//` comments; the
  TextMate directive set is now the real one (no `defval`; `def`,
  `trydefine`, `redef`, `trydefenv(s)`, prefixed includes, indented
  directives).
- **Untrusted workspaces** additionally restrict
  `kamailioLsp.serverPath` and `kamailioLsp.modulesPath`, closing a
  workspace-settings code-execution vector.
- Docs: the diagnostics demo misspells a real Kamailio parameter
  (`fr_tmer` vs tm's `fr_timer`) and quotes the message the squiggle
  actually shows (`Can't set module parameter`); realistic module
  counts; assorted wording fixes.

## [0.1.1] — 2026-08-20

- Documentation: the check flag is Kamailio's lowercase `-c`
  everywhere (a leftover uppercase `-C` was OpenSIPS's flag), and the
  example diagnostic now quotes Kamailio's real message
  (`parameter <fr_timeot> of type <2:int> not found in module <tm>`).

## [0.1.0] — 2026-08-20

- Initial release: diagnostics via `kamailio -c --all-errors`
  (line/column ranges and multi-line spans, versioned against the
  buffer, serialized, time-bounded, output-capped),
  context-sensitive completion (modules, parameters, exported
  functions of loaded modules, core functions/parameters,
  pseudo-variables after `$`), hover documentation, go-to-definition
  for routes, and document symbols.
- Documentation harvested from a Kamailio source tree
  (`src/modules/<name>/README`) and a kamailio-wiki checkout
  (core cookbook + pseudo-variables), cached per (tree, wiki) pair.
- Workspace-trust gate: diagnostics stay off in untrusted folders
  (checking a config loads its modules); settings changes restart
  the server live.
- Static snippets for route blocks, preprocessor directives,
  `loadmodule`/`modparam`, control flow, `xlog`, `sl_send_reply`;
  function completions insert tabstop snippets.
- **Signature help**: the innermost unclosed call's signature with
  the active parameter highlighted, on `(` and `,` — module exports
  first, then core functions (commas inside strings don't advance the
  parameter).
- **References, rename, highlights** for route names: Shift+F12
  lists every call site and the definition, F2 renames everywhere
  (quoted call sites rewritten inside the quotes, illegal names
  rejected — `:` is legal for event-route names), occurrences
  highlight with the definition as a write.
- **Include awareness**: `include_file`/`import_file` are followed
  (open buffers preferred over disk, cycle-safe, capped) — completion
  sees included modules and routes, and go-to-definition jumps into
  the include. Dotted and colon-carrying route names resolve whole.
- **Instant analyzer warnings** between saves, debounced as you
  type: undefined `route()` targets and duplicate route definitions.
  Setting `kamailioLsp.diagnostics.analyzer`.
- **Folding** for every route-family block, and the
  outline/breadcrumbs carry full block extents (nested document
  symbols).
- Completion quality: duplicate labels collapse to the most
  informative item, a typed `$token` is replaced instead of doubled,
  and `route(` completes route names.
- Tree-sitter grammar (`tree-sitter-kamailio`) with corpus tests.
- Version-proven against Kamailio 6.0.x (binary 6.0.1, branch 6.0
  tree, wiki cookbooks 6.0.x).
