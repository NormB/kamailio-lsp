# Changelog

All notable changes to the Kamailio Routing Script extension.

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
