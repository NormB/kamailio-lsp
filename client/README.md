# Kamailio Routing Script

Language support for the [Kamailio](https://kamailio.org) routing
script (`kamailio.cfg`) — **the real Kamailio parser checks your
config as you work**, and completion, hover documentation, and
navigation come from the actual documentation of your Kamailio
version. Platform builds bundle the language server: install the
extension and it just works.

## Features

- **Diagnostics you can trust** — every save runs `kamailio -c`, so
  the squiggles are the *real* parser's verdict, at the exact line
  and column, for exactly your Kamailio version — misspell a modparam
  name (`fr_tmer` for tm's `fr_timer`) and that line is flagged with
  `Can't set module parameter`.
- **Completion that knows context**
  - `loadmodule "` → every module in your source tree
  - `modparam("tm", "` → tm's parameters, with their documentation
  - inside a route → exported functions of the modules you loaded,
    plus core functions, parameters, route names, and keywords
  - `$` → pseudo-variables (`$ru`, `$si`, …) with descriptions
  - functions insert as snippets — the cursor lands between the
    parentheses
- **Hover documentation** for functions, parameters, modules, and
  pseudo-variables, harvested from Kamailio's own docs.
- **Navigation** — go to definition: Ctrl+Click `route(NAME)` to jump
  to its definition (even into an `include_file`); Ctrl+Shift+O lists
  every route block (document symbols) with its full extent; route
  blocks fold.
- **Signature help** — type `(` or `,` in a call and the signature
  pops up with the active parameter highlighted.
- **References & rename** — Shift+F12 lists every call site of a
  route; F2 renames it everywhere, quoted call sites included.
- **Instant warnings** — undefined `route()` targets, duplicate
  route definitions, and modparam names your source tree does not
  document are flagged as you type, no save needed.
- **Workspace symbols & code lenses** — Ctrl+T finds any route across
  open files and includes; callable routes show reference counts.
- **Quick fixes** — the lightbulb loads the module exporting an
  unknown command, or creates a stub for an undefined route.
- **Semantic highlighting** — route names and pseudo-variables get
  consistent theme colors, inside strings too.
- **Formatting** — Shift+Alt+F or format-on-save re-indents by brace
  depth and strips trailing whitespace, following your editor's tab
  settings; range formatting does the same for a selection. Line-preserving on purpose: it never joins, splits or
  reorders lines, never touches a string or comment, and leaves a
  line that continues the previous statement exactly where you put
  it — a hanging indent or a braceless `if` body stays readable.
- **Call hierarchy** — Shift+Alt+H on a route name opens the route
  call graph: who calls `route[X]`, and what `route[X]` calls, across
  the include closure.
- **Inlay hints** — parameter names from the documentation drawn at
  each documented call site, plus what each `#!define` expands to at its uses.
- **Include links** — Ctrl+Click an `include_file`/`import_file` path
  to open it.
- **Preprocessor symbols** — hover a `#!define` name for what it
  binds, Ctrl+Click to its directive (works inside `#!ifdef` too),
  and find them in the outline. Routes reached through an alias are
  no longer wrongly flagged as undefined.
- **Pull diagnostics** — `textDocument/diagnostic` and a workspace
  sweep that reports problems across the project without opening
  every file. Only root configs are swept: a config another one
  includes is a fragment, not a program.
- **Watched files** — an include or the documentation tree changing
  on disk (a git checkout, a rebuild) re-checks and re-harvests
  without you touching the buffer.
- **Refactorings** — extract a selection into a `route[...]` of its
  own, leaving a call behind; remove duplicate `loadmodule` lines (a
  second load is a parse error, not untidiness). The extraction
  declines when it would change behaviour — a selection containing
  `return` is never lifted.
- **Live settings** — the runtime toggles apply without restarting
  the server.
- **Core language out of the box** — parameters, functions and
  pseudo-variables (`debug`, `children`, `listen`) complete before you configure
  anything, from a catalogue harvested from Kamailio 6.1.x and shipped
  with the extension. Hover says which version it came from; point
  `kamailioWiki` at your own wiki checkout for docs exact to your build.
- **Snippets** — `route`, `failure_route`, `ifmethod`, `modparam`,
  `switch`, `xlog`, and more.
- **Safe by default** — in untrusted workspaces diagnostics stay off
  (checking a config loads its modules); everything else keeps
  working.

## Quick start

1. Install this extension (the platform packages bundle the server —
   Linux, macOS, and Windows, x64 and arm64).
2. Open a folder containing a `kamailio.cfg` — syntax colors,
   completion, and navigation work immediately. Also recognized:
   `kamailio*.cfg` and `*.kamailio.cfg` (so `kamailio-local.cfg` and
   split configs work), and **any file whose first line is a Kamailio
   script-type marker** — `#!KAMAILIO`, `#!OPENSER`, `#!SER`,
   `#!MAXCOMPAT` or `#!ALL` — whatever it is called. The generic
   `.cfg` extension is deliberately left alone so unrelated tools'
   config files are not hijacked; for anything else, add a
   [`files.associations`](https://code.visualstudio.com/docs/languages/identifiers)
   entry mapping it to `kamailio-cfg`.
3. For live error checking, point
   **Settings → Kamailio Lsp: Kamailio Path** at your `kamailio`
   binary and save the file.
4. For the richest completion docs, set **Kamailio Src** to an
   Kamailio source tree matching your version.

New to all of this? The step-by-step
[Getting Started guide](https://github.com/NormB/kamailio-lsp/blob/main/docs/GETTING_STARTED.md)
covers installation and usage click by click.

## Settings

| Setting | Default | Effect |
|---|---|---|
| `kamailioLsp.enable` | `true` | Master switch. |
| `kamailioLsp.serverPath` | bundled | Server binary override. |
| `kamailioLsp.kamailioPath` | `kamailio` | Binary for `-c` diagnostics; empty disables. |
| `kamailioLsp.kamailioSrc` | — | Source tree for module completion/hover docs. |
| `kamailioLsp.kamailioWiki` | — | kamailio-wiki checkout for core-language docs. |
| `kamailioLsp.modulesPath` | — | Module search path for the checker (`-L`). |
| `kamailioLsp.diagnostics.enable` | `true` | Toggle checks without losing the path. |
| `kamailioLsp.diagnostics.analyzer` | `true` | As-you-type analyzer warnings. |
| `kamailioLsp.diagnostics.maxProblems` | `100` | Diagnostics cap per file. |
| `kamailioLsp.checkTimeoutMs` | `10000` | Bound on one `-c` run. |
| `kamailioLsp.codeLens.references` | `true` | Reference-count code lenses. |
| `kamailioLsp.completion.snippets` | `true` | Function completions as snippets. |
| `kamailioLsp.cacheDir` | platform | Documentation-cache location. |
| `kamailioLsp.trace.server` | `off` | LSP traffic tracing. |

Full reference:
[Features & Settings](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md).

## Requirements

None to start — the server is bundled. Optional, for the full
experience: an `kamailio` binary (diagnostics) and a Kamailio source
tree plus a kamailio-wiki checkout (documentation). Version-proven
against Kamailio 6.1.x and 6.0.x.

## Links

[Repository](https://github.com/NormB/kamailio-lsp) ·
[Getting Started](https://github.com/NormB/kamailio-lsp/blob/main/docs/GETTING_STARTED.md) ·
[Admin Guide](https://github.com/NormB/kamailio-lsp/blob/main/docs/ADMIN.md) ·
[Issues](https://github.com/NormB/kamailio-lsp/issues) ·
[Releases](https://github.com/NormB/kamailio-lsp/releases)

Dual-licensed MIT or Apache-2.0.
