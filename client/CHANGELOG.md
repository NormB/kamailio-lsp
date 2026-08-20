# Changelog

All notable changes to the Kamailio Routing Script extension.

## [0.1.0] — unreleased

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
- Tree-sitter grammar (`tree-sitter-kamailio`) with corpus tests.
- Version-proven against Kamailio 6.0.x (binary 6.0.1, branch 6.0
  tree, wiki cookbooks 6.0.x).
