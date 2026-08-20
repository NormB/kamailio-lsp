# Features, Settings & Snippets

## Features

#### Diagnostics

Two complementary layers:

- **Parser diagnostics** — your `kamailio.cfg` is checked by the
  **real Kamailio parser** (`kamailio -c --all-errors`) on open and
  save; errors appear as squiggles at the exact line and column (or
  column range, or multi-line span) the parser reports. Failed or
  timed-out checks clear stale squiggles instead of leaving them
  pinned; results are versioned against the buffer they were computed
  for; runs are serialized, time-bounded, and output-capped.
  Automatically off in untrusted workspaces (checking a config loads
  its modules, which executes code).
- **Analyzer warnings** — fast, between saves, as you type
  (debounced): `route(NAME)` calls whose target is defined nowhere in
  the file or its includes, and duplicate route definitions. Source
  `kamailio-lsp`, severity warning; toggle with
  `kamailioLsp.diagnostics.analyzer`.

#### Completion

Context-sensitive, from documentation harvested out of your Kamailio
source tree (result cached per tree):

| You type | You get |
|---|---|
| `loadmodule "` | every module in the tree |
| `modparam("` | module names |
| `modparam("tm", "` | tm's parameters, each with docs |
| letters in a route | exported functions of **loaded** modules, core functions, core parameters, route names, keywords |
| `route(` | route names (this file and its includes) |
| `$` | pseudo-variables with descriptions (the typed `$word` is replaced, never doubled) |

Function completions insert **snippets** — the cursor lands between
the parentheses (`t_relay(│)`) — disable with
`kamailioLsp.completion.snippets`. Duplicate labels are collapsed,
keeping the most informative item. Modules loaded and routes defined
in `include_file`/`import_file` files count — the closure is followed
(open editor buffers preferred over disk).

#### Signature help

Type `(` or `,` inside a call and the function's signature pops up
with the active parameter highlighted — module exports first, then
core functions. Commas inside strings don't advance the parameter.

#### Hover, navigation, outline

Hover any function/parameter/module/`$variable` for its
documentation; **Ctrl+Click** a `route(NAME)` reference to jump to
its definition — including definitions that live in an included file;
**Ctrl+Shift+O** lists every route block with its full extent (the
outline nests, and breadcrumbs know which block you're in).

#### References, rename, highlights

**Shift+F12** lists every call site and the definition of a route
name; **F2** renames a route everywhere (quoted call sites are
rewritten inside the quotes; illegal names are rejected — the legal
charset includes `:` for `event_route` names); all occurrences of the
route under the cursor are highlighted, the definition as a write.

#### Folding

Every route-family block folds (`request_route`, `failure_route[x]`,
`event_route[...]`, ...), with string- and comment-safe brace
matching.

#### Static snippets

Type a prefix and press Tab: `request_route`, `route`,
`failure_route`, `onreply_route`, `branch_route`, `event_route`,
`onsend_route`, `loadmodule`, `modparam`, `define`, `ifdef`,
`ifmethod`, `ifelse`, `while`, `switch`, `xlog`, `sl_send_reply`.

## Settings

VS Code settings (Ctrl+, → search "kamailio"); other editors pass the
initialization option; environment variables are the fallback for
clients that can't pass options.

| VS Code setting | Init option | Environment | Default | Effect |
|---|---|---|---|---|
| `kamailioLsp.enable` | — | — | `true` | Master switch for the extension. |
| `kamailioLsp.serverPath` | — | — | `kamailio-lsp` | Server binary; the default uses the copy bundled in platform builds, then PATH. |
| `kamailioLsp.kamailioPath` | `kamailioPath` | `KAMAILIO_LSP_BIN` | `kamailio` | Binary for `-c` diagnostics; empty disables them. |
| `kamailioLsp.kamailioSrc` | `kamailioSrc` | `KAMAILIO_LSP_SRC` | *(unset)* | Source tree for module completion/hover docs. |
| `kamailioLsp.kamailioWiki` | `kamailioWiki` | `KAMAILIO_LSP_WIKI` | *(unset)* | kamailio-wiki checkout for core-language docs. |
| `kamailioLsp.modulesPath` | `modulesPath` | — | *(unset)* | Module search path for the checker (`-L`). |
| `kamailioLsp.diagnostics.enable` | *(maps to empty `kamailioPath`)* | — | `true` | Toggle diagnostics without losing the configured path. |
| `kamailioLsp.diagnostics.analyzer` | `analyzerDiagnostics` | — | `true` | Fast analyzer warnings between saves (undefined `route()` targets, duplicate definitions). |
| `kamailioLsp.diagnostics.maxProblems` | `maxDiagnostics` | — | `100` | Bound on published diagnostics per file. |
| `kamailioLsp.checkTimeoutMs` | `checkTimeoutMs` | `KAMAILIO_LSP_CHECK_TIMEOUT_MS` | `10000` | Kill a `-c` run after this many ms. |
| `kamailioLsp.completion.snippets` | `snippetCompletions` | — | `true` | Function completions as tabstop snippets. |
| `kamailioLsp.cacheDir` | `cacheDir` | `KAMAILIO_LSP_CACHE_DIR` | platform cache dir | Documentation-catalog cache location. |
| `kamailioLsp.trace.server` | — | — | `off` | LSP traffic tracing in the output channel. |
| — | — | `KAMAILIO_LSP_OUTPUT_CAP_BYTES` | `1048576` | Byte cap on captured `-c` output. |

## Notes

- The analyzer resolves `route(NAME)` literally: a route addressed
  through a `#!define` alias (`#!define RELAY 1` + `route(RELAY)`)
  can be flagged as undefined even though the preprocessor expands it
  — silence such spots by naming the route directly or disabling
  `kamailioLsp.diagnostics.analyzer`.
- Include handling is capped for safety: depth 8, 64 files, 1 MiB per
  file; relative paths resolve against the including file's
  directory. `KAMAILIO_LSP_ANALYZER_DEBOUNCE_MS` tunes the analyzer
  debounce (default 300).
- Server-backed options apply at initialization; the VS Code client
  restarts the server automatically when any `kamailioLsp.*` setting
  changes, so edits take effect immediately.
- Snippet completions and static snippets compose: static snippets
  scaffold blocks, completion snippets fill in calls.
