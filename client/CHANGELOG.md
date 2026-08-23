# Changelog

All notable changes to the Kamailio Routing Script extension.

## [0.4.1] — 2026-08-23

- **Documentation drift gate**: the README and the features page are
  now checked against the server itself — every capability advertised
  in `initialize`, every initialization option and environment
  variable the server reads, and every VS Code setting the client
  contributes must appear in the docs, or the build fails.
- Fixes the drift that gate found: the README never mentioned include
  links (Ctrl+Click on `include_file`/`import_file`), `prepareRename`,
  `semanticTokens/range`, or live reconfiguration — all shipped
  earlier and all invisible to anyone reading the front page.
- Documents `KAMAILIO_LSP_TRACE_INDEX`, an environment variable the
  server has read all along and no page mentioned.

## [0.4.0] — 2026-08-23

- **Language-server stack moved to `tower-lsp-server`**: `tower-lsp`
  has had no release since 0.20.0 (August 2023); the maintained
  community fork replaces it. No behaviour changes — the protocol
  suite (raw JSON-RPC over stdio) passes unchanged, and
  `workspace/symbol` still answers with the same array on the wire.
  Dropping the `url` crate takes its whole ICU/idna tree with it: the
  server now builds from 66 dependencies instead of 95.
- **Version-proven against Kamailio 6.1.4**, the current stable line,
  alongside 6.0.x: a real 6.1.4 tree, a fresh kamailio-wiki checkout,
  and a 6.1.4 binary. Every behavioural citation in the suite was
  re-verified against that binary, and the 6.1.4 diagnostic shapes
  are captured verbatim beside the 6.0.1 ones.
- Nothing pins a Kamailio version: module docs come from the tree you
  point at and core docs from the newest stable cookbook in your wiki
  checkout — a selection that is now covered by a test (newest stable
  wins, `devel` never does).

## [0.3.0] — 2026-08-20

- **Per-version memoization** (internal): the per-document
  computations (route blocks, references, semantic spans) are cached
  per (document, version) and shared by the hot handlers — semantic
  tokens (full and range), code lenses, document symbols, references
  — instead of being recomputed per request; stale versions evict in
  place and closed documents drop their entry.
- **Semantic tokens range**: `textDocument/semanticTokens/range` is
  served (and advertised) alongside `full` — editors can request
  just the visible slice of a large config.
- **Grammar↔scanner drift gate** (internal): corpus-driven tests
  prove the tree-sitter grammar and the server's scanner agree on
  route definitions (total and named), `loadmodule`, and `modparam`
  counts; extending one without the other fails CI.
- **Prepare rename**: F2 is validated server-side before the rename
  box opens — exact range + placeholder on renamable route names,
  blocked outright off-symbol and on per-kind names (`event_route`
  etc.) that cannot be renamed.
- **Include links**: `include_file`/`import_file` paths are
  clickable document links, resolved against the including file's
  directory (absolute paths pass through; missing targets still
  link).
- **Dynamic settings**: changing `diagnostics.analyzer`,
  `completion.snippets`, `codeLens.references`,
  `diagnostics.maxProblems`, or `checkTimeoutMs` no longer restarts
  the language server — the client pushes
  `workspace/didChangeConfiguration` and the server applies the
  change live, republishing diagnostics for open files. Path
  settings (server/kamailio binary, source/wiki trees, cache dir)
  still restart automatically.
- **Content-aware catalog cache**: the harvest cache is now keyed by
  a manifest of every file it reads (size + mtime per module README
  and cookbook page) plus a schema version — editing a doc file
  re-harvests; before, only directory-level changes did.
- **Harvest status**: the post-initialize harvest reports LSP
  `workDoneProgress` (busy indicator in supporting editors), and a
  configured source/wiki tree that yields zero symbols raises a
  visible warning naming the path.
- **Checker cwd parity**: the server now runs `kamailio -c` from the
  configuration's own directory (as the CLI always did), so relative
  `include_file`/`import_file` paths resolve identically in editor
  and CI runs.
- **Latest-wins checks**: a newer save supersedes an in-flight
  `kamailio -c` run on the same document — the stale child process
  is killed instead of blocking the fresh check behind it.

## [0.2.1] — 2026-08-20

- Internal: drift gates for the release workflow; ground-truth
  re-audit against the current Kamailio 6.0 branch head and the
  packaged 6.0.1 parser (no cfg.y/cfg.lex drift, all gates green).
  No functional changes.

## [0.2.0] — 2026-08-20

- **Workspace symbols**: Ctrl+T searches route definitions across
  every open file and its include closure (case-insensitive,
  capped).
- **Code lenses**: named callable `route` blocks show a reference
  count (closure-wide); `failure_route` and friends carry none —
  their arming isn't `route()`-countable. Setting
  `kamailioLsp.codeLens.references`.
- **Quick fixes**: "Load module 'X'" when the parser says
  `unknown command, missing loadmodule?` (Kamailio names no function
  in the message, so it is read from the call under the diagnostic)
  and "Create route[x]" (stub with an `exit;` body) for undefined
  targets.
- **Catalog-pinned validation**: modparam parameters the configured
  source tree does not document warn as you type; unknown modules
  stay silent. Found and fixed a real harvest gap along the way:
  presence-style `db_url(str)` headings (no space before the type)
  now parse.
- **Semantic tokens** for route names and pseudo-variables —
  including inside single- OR double-quoted strings, which produce
  the same STRING token (cfg.lex) and interpolate identically.
- **CLI**: `kamailio-lsp check [--strict] [--bin <kamailio>]
  <file>...` — analyzer + real-parser findings as grep-able
  `file:line:col` lines with include-error remapping; exit codes
  0/1/2 for CI and git hooks.

## [0.1.3] — 2026-08-20

- **References and rename cross the include closure**: renaming a
  route defined in an `include_file` rewrites every file, and
  Shift+F12 lists the include's definition.
- **Route namespaces**: `route(X)` is satisfied only by `route[X]`
  blocks (Kamailio keeps per-kind route tables — a `failure_route[X]`
  no longer masks an undefined target); duplicate-definition warnings
  are per kind; purely numeric `route(N)` calls are runtime dispatch
  and never warn; `route(` completion offers only callable names;
  renaming `failure_route`/`branch_route`/... names is rejected with
  an explanation (their call sites are module-function arguments).
- **Single-quoted strings** (`loadmodule 'tm.so'`, `route['SQ']`,
  `xlog('hi')`) are recognized everywhere: analysis, completion
  contexts, tree-sitter, and TextMate highlighting — with Kamailio's
  real semantics (no escapes, may span lines).
- **Signature help labels** split on top-level commas only — nested
  calls, bracket groups (`t_relay([host, port])`), and quoted commas
  stay whole.
- Release workflow: a failed Open VSX publish no longer bypasses the
  rerun tolerance (exit-status capture under `bash -e`).

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
