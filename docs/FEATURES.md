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
  for; runs are serialized, time-bounded, and output-capped, and a
  newer save supersedes an in-flight check (the stale child process
  is killed — latest wins). Checks run from the config's own
  directory, so relative includes resolve as in the CLI.
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

#### Workspace symbols, code lenses

**Ctrl+T** searches route definitions across every open file and its
includes (case-insensitive, capped at 256). Named `route` blocks show
a **reference count** code lens (counted across the include closure;
only `route()`-callable blocks get one — `failure_route` and friends
are armed via module functions we don't count); disable with
`kamailioLsp.codeLens.references`.

#### Quick fixes

The lightbulb offers: **Load module 'X'** when the parser reports
`unknown command, missing loadmodule?` and the catalog knows which
module exports the function called at that spot (inserted after the
last `loadmodule`, any load form), and **Create route[x]** for an
undefined `route(x)` target (a stub with an `exit;` body is appended
— empty route bodies are not valid Kamailio).

#### Catalog-pinned validation

`modparam("m", "p", ...)` (and `modparamx`) warns as you type when
the configured source tree documents module `m` but no parameter `p`
— version-exact by construction, since the catalog IS your pinned
tree. Unknown modules stay silent.

#### Semantic highlighting

Route names (definitions and call sites) and pseudo-variables get
semantic tokens, so themes color them consistently — including pvars
inside strings of either quote style (both produce the same STRING
token and modules interpolate the value at fixup); comments and
`#!` directives never light up.

#### CLI check mode

`kamailio-lsp check [--strict] [--bin <kamailio>] <file>...` runs the
same analyzer (plus the real `kamailio -c --all-errors` when a binary
is given) for CI pipelines and git hooks. Findings print as
`file:line:col: severity: message` (1-based); errors inside included
files attach to the `include_file` directive. Exit codes: 0 clean,
1 findings (or warnings under `--strict`), 2 usage.

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
| `kamailioLsp.diagnostics.analyzer` | `analyzerDiagnostics` | — | `true` | Fast analyzer warnings between saves (undefined `route()` targets, duplicate definitions, undocumented modparams). |
| `kamailioLsp.codeLens.references` | `codeLensReferences` | — | `true` | Reference-count code lenses on route definitions. |
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
- Runtime toggles (`diagnostics.analyzer`, `completion.snippets`,
  `codeLens.references`, `diagnostics.maxProblems`,
  `checkTimeoutMs`) apply **live**: the VS Code client pushes them to
  the running server via `workspace/didChangeConfiguration` and open
  documents republish immediately. Settings that shape
  initialization (`serverPath`, `kamailioPath`, `kamailioSrc`,
  `kamailioWiki`, `modulesPath`, `cacheDir`, `enable`,
  `diagnostics.enable`) still restart the server automatically.
- Snippet completions and static snippets compose: static snippets
  scaffold blocks, completion snippets fill in calls.
