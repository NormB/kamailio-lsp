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
| letters in a route | exported functions of **loaded** modules, core functions, core parameters, route names, keywords (documented ones carry their text) |
| `route(` | route names (this file and its includes) |
| `$` | pseudo-variables with descriptions (the typed `$word` is replaced, never doubled) |
| `listen=udp:… ` | the three modifiers a `listen` line takes — `advertise`, `name`, `virtual` |
| inside `socket = { ` | the seven attributes the brace form takes — `bind`, `advertise`, `name`, `agname`, `workers`, `virtual`, `vrf`. Membership comes from the grammar, so `workers` is offered even though the cookbook's attribute list omits it, and says so |
| `xlog(` | the log levels, quoted — `"L_INFO"`, because the fixup takes a string there. Type `xlog("` and they come unquoted instead. `xlogl` and `xlogm` too; `xdbg`, `xinfo`, `xerr`, `xnotice` and `xwarn` carry their level in the name and take a format alone. The set is read from the `switch` in your tree's `src/modules/xlog/xlog.c`, so it is the set *your* release accepts — kamailio takes `L_BUG`, and the three-argument `xlog(facility, level, format)` form is not offered at its level position |

Function completions insert **snippets** — the cursor lands between
the parentheses (`t_relay(│)`) — disable with
`kamailioLsp.completion.snippets`. Duplicate labels are collapsed,
keeping the most informative item. Modules loaded and routes defined
in `include_file`/`import_file` files count — the closure is followed
(open editor buffers preferred over disk).

#### Turning the popups off

<kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd>
(<kbd>Cmd</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> on macOS) stops hovers and
completion; the same keys bring them back. The status bar reads
`Kamailio hints off` meanwhile. It applies at once — no restart — and
leaves diagnostics alone, which have their own switch. The setting
behind it is `kamailioLsp.assistance`.

#### Signature help

Type `(` or `,` inside a call and the function's signature pops up
with the active parameter highlighted — module exports first, then
core functions. Commas inside strings don't advance the parameter.

#### Hover, go to definition, document symbols

Hovering a core parameter shows its description and the cookbook's
worked example — the example is where the form is. The default is
already part of the description here, because this cookbook writes it
into the first paragraph ("Default value is 8." in `children`) rather
than on a line of its own.

On a `listen =` line the modifiers after the address hover, and
inside a `socket = { ... }` block its attributes do. The two are
different syntaxes with different sets, so each answers only where it
applies — `name` and `virtual` are ordinary words elsewhere.

Hover any function/parameter/module/`$variable` for its
documentation — and any routing block (`request_route`,
`branch_route`, `event_route` and the rest) or control statement
(`if`, `switch`, `while`, `else`), which come from the cookbook's
"Routing Blocks" and "Script Statements" sections; **Ctrl+Click** a
`route(NAME)` reference to jump to
its definition — including definitions that live in an included file;
**Ctrl+Shift+O** lists every route block with its full extent (the
outline nests, and breadcrumbs know which block you're in).

#### References, rename, highlights

**Shift+F12** lists every call site and the definition of a route
name; **F2** renames a route everywhere (quoted call sites are
rewritten inside the quotes; illegal names are rejected — the legal
charset includes `:` for `event_route` names); all occurrences of the
route under the cursor are highlighted, the definition as a write.

**Prepare rename** (`textDocument/prepareRename`): before the rename
box even opens, the server validates the position — on a renamable
main-table route name it returns the exact symbol range and the
current name as placeholder; off-symbol, or on names that cannot be
renamed (`event_route` and other per-kind names, which are armed
from module-function string arguments), the editor blocks F2
outright instead of failing afterwards.

#### Include links

Every `include_file`/`import_file` path (bare or `#!`/`!!`-prefixed,
either quote style) becomes a clickable **document link**
(`textDocument/documentLink`): relative paths resolve against the
document's own directory — the same rule the checker and the include
closure use — absolute paths pass through, and links are produced
even for files that do not exist yet (the editor reports the miss on
click). Paths in comments or strings never link.

#### An included file opened on its own

A config another config includes is a **fragment**, not a program.
Checked on its own it flags every route its parent defines as
undefined and reports every construct it continues as a syntax error,
and `kamailio -c` was never meant to be handed one. The workspace
sweep has always known this and skipped fragments — but opening one
directly used to produce exactly those errors, as artefacts of how
the file was opened rather than of anything wrong with it.

So a fragment is answered in its **root's** context. Given

```
/etc/kamailio/
├── kamailio.cfg          #!KAMAILIO
│                         include_file "modules.cfg"
│                         include_file "routing/inbound.cfg"
├── modules.cfg
└── routing/
    ├── inbound.cfg       route[INBOUND] { route(SEND_TO_CARRIER); }
    └── carriers.cfg      route[SEND_TO_CARRIER] { … }
```

opening `routing/inbound.cfg` on its own resolves `SEND_TO_CARRIER`,
offers it in completion, and reports nothing about it — even though
`inbound.cfg` neither defines it nor includes the file that does.

**The only thing the user must do is open the folder** (in VS Code,
`File → Open Folder…`), because the root is found by reading the
configs under the client's workspace folders. A client that opened a
single file has given the server nothing to read; the root could be
any directory above it, and guessing is worse than saying so. With no
folder, every config is a program of its own — the pre-0.15.0
behaviour.

The server keeps the workspace's include graph inverted — which
config names which — and climbs it to the top of the chain that
reaches the open document:

- **Diagnostics.** The analyzer runs over the root's closure, so the
  routes, modules and `#!define`s the parent brings exist; only
  findings that land in the fragment are reported. `kamailio -c` is
  run on the ROOT, and each error it reports is routed to the file it
  actually names — an error inside the fragment lands on the
  fragment's own line, and the root's own errors stay on the root.
- **Navigation and completion.** Go to definition, references,
  rename, call hierarchy, workspace symbols and route completion all
  span the root's closure, so a `route()` the parent defines resolves
  from inside the fragment and is offered while typing there.
- **`kamailio/analysisRoot`.** A non-LSP request answering "what is
  this document a piece of": the root's URI, or `null` when the
  document is a program in its own right — or a file the workspace
  never includes. The VS Code client uses it to decide whether an
  unassociated `.cfg` on screen is part of a Kamailio configuration
  (see **Notes**).

A fragment reached from more than one root has no single true answer;
the lexicographically first parent is taken at each step, so the
context cannot flicker between edits. Include cycles terminate at the
last config not already visited.

`.` and `..` are folded when an include is resolved, so one file has
one name: a per-site layout that reaches shared routing as
`../common/routing.cfg` names the same file the editor opens as
`common/routing.cfg`. Without that they are two keys — the fragment
has no root and the closure visits it twice, so every route it
defines reads as defined more than once. The folding is textual, not
`canonicalize`: an include may name a file that does not exist yet,
and canonicalising would replace the path you see with the target of
any symlink on it. The one case where the two differ is a symlinked
directory, where the OS reads `sites/../common` relative to the
link's target and this does not.

The graph is rebuilt when a config is created, deleted or changed on
disk, when a document opens or closes, and when an edit adds or
removes an include directive — nothing else can move a file from one
root to another, so ordinary typing does not pay for it. **Typing the
`include_file` line re-checks the file it names**, if that file is
open: adding the include is the fix for the warnings that file was
showing, and leaving them up until it is next touched makes the fix
look like it did not work. A deeper change than one level corrects
itself on save, through the watched-files path.

The closure itself is bounded too — depth 8, 64 files — and a
configuration with one include per carrier passes 64 without trying.
A fragment past the bound is not in the closure built from its root,
so its OWN closure leads and the root's follows: analysing a file in
its parent's context must only ever ADD to what that file could
already see, never take its own includes away.

Answering a follow-up about a route defined in the root needs the
root's text, and while you are editing an include the root is not a
file the editor has opened. It is read from disk (same 1 MiB cap as
the include loader) rather than treated as empty — reading it as
empty is how call hierarchy came to answer "nobody calls this" for a
route the buffer on screen calls two lines up.

What counts as "a config this server can read" is decided in ONE
place, because two places is a disagreement: a file over 1 MiB is
skipped by both the graph and the closure, so a root can never be
found and then refuse to load, and bytes are decoded leniently, so a
latin-1 accent in a comment does not erase a configuration and every
fragment it includes along with it. A config the graph could not read
is named in the log, not dropped in silence.

The **CLI knows this too.** `kamailio-lsp check` is what a git hook and a
CI job run, and it reaches the same conclusion the editor does: the
directory holding the files it was given is its workspace (plus the
three directories above, read but not walked, because the root that
includes `inc/routes.cfg` is usually the config one level up). Pass it
a fragment alone and it still analyses it as part of its root —
before, it reported the parent's routes as undefined, and `--strict`
turned that into a failed build for a correct configuration.

**Folders added to the workspace after startup count.** The server
advertises `workspaceFolders` support with change notifications and
rebuilds the graph when one is added or removed; without that, "Add
Folder to Workspace" left every fragment in the new folder
unrecognised until the window was reloaded.

**A conditionally-compiled include is not claimed.** Kamailio's
preprocessor decides whether an `include_file` is read at all: a file
inside a `#!ifdef` for a symbol nobody defined is never opened, and
the checker does not even report syntax errors in it. Treating such a
file as part of that configuration is worse than treating it as part
of nothing — it would be analysed against a program it is not in, and
checked through a root that never reads it, so its own errors would
never surface. A conditional counts as holding only when the symbol
is defined earlier in the same file; anything less certain yields
nothing, which is simply the behaviour from before this feature
existed. (OpenSIPS differs here — its 4.0.1 parser reads the include
either way — so the sibling server deliberately does not do this.)

The scan behind the graph is bounded at 500 configs and **says so in
the log** when it stops early. Past that bound a root is simply not
seen and its fragments stop being recognised — no colours, no
context — so a silent bound would be a disappearance with no
explanation anywhere.

#### Workspace symbols, code lenses

**Ctrl+T** searches route definitions across every open file and its
includes (case-insensitive, capped at 256). Named `route` blocks show
a **reference count** code lens (counted across the include closure;
only `route()`-callable blocks get one — `failure_route` and friends
are armed via module functions we don't count); disable with
`kamailioLsp.codeLens.references`.

#### Preprocessor symbols

`#!define` and its relatives bind names the preprocessor substitutes
textually, before the parser sees anything. The server now reads them:

- **Hover** a name for what it binds and which directive bound it.
- **Ctrl+Click** it to jump to that directive — including from an
  `#!ifdef` operand, which is not code position.
- **Completion** offers them wherever code is legal.
- The **outline** lists them alongside the route blocks.
- The **analyzer** expands through them, so a route reached by alias
  is no longer flagged as undefined.

Every defining form kamailio's `src/core/cfg.lex` recognises is
collected, behind either prefix (`#!` or `!!`): `define`/`def`,
`trydefine`/`trydef`, `redefine`/`redef`, `defexp`, `defexps`,
`defenv`, `defenvs`, `trydefenv`, `trydefenvs`, plus `substdef` and
`substdefs` — whose delimited form (`#!substdef "!NAME!value!g"`)
binds a name as well. A backslash-continued directive is read whole,
so its continuation lines are part of the value.

A define outranks a same-named module symbol in hover and completion.
That is not a preference: the substitution happens first, so the
define is what the config actually means.

#### Inlay hints

Two kinds, each independently switchable.

**Parameter names.** Arguments at a documented call site are labelled
with the name the module's own documentation gives them, so
`t_relay("udp", 1)` reads as `t_relay(flags: "udp", outbound_proxy: 1)`
without the document changing. Only calls the catalogue knows are
hinted, which is what keeps `if`, `while` and `route` out without
special-casing them. A call carrying more arguments than the signature
documents is hinted as far as the signature goes and no further —
guessing past the end would be inventing names. Signatures are written
for humans (`[flags]`), so bracket markers, defaults and leading types
are stripped to the name; a parameter that reduces to nothing is
skipped rather than drawn as an empty chip.

**Preprocessor values.** Each use of a `#!define`d symbol carries what
it expands to, including inside `#!ifdef`, where the operand is a
directive token rather than code. The definition site is not hinted —
`#!define PORT 5060` already says what it binds — and a define with no
value has nothing to show.

The editor asks for the visible range and only that range is computed.
`kamailioLsp.inlayHints.parameterNames` and
`kamailioLsp.inlayHints.defineValues` turn them off; both apply live.

#### Call hierarchy

**Shift+Alt+H** on a route name — at a `route(NAME)` call or on the
`route[NAME]` definition — opens the call graph. Incoming calls list
every block that calls it; outgoing calls list every route it calls.
Both span the include closure, so a caller living in an included file
shows up with that file's URI.

Several calls from the same block collapse into one entry carrying
each call site's range, so the editor can step through them.

A route called but defined nowhere still appears as an outgoing edge,
marked `undefined` — the call is in the file, and dropping it would
hide something the reader can see.

The graph is the **main route table**. `route(NAME)` is the only call
form the server can observe, so `route[NAME]` blocks are what take
part. `request_route` and the per-kind blocks
(`failure_route[NAME]`, `event_route[NAME]`, …) are entry points
armed by the core or by a module function that takes the route name
as a string (`t_on_failure("NAME")`), which the server does not
track: those blocks can *make* calls, and do show up as callers, but
asking for their own hierarchy declines rather than reporting "no
callers" — which would be a confident wrong answer.

#### Quick fixes

The lightbulb offers: **Load module 'X'** when the parser reports
`unknown command, missing loadmodule?` and the catalog knows which
module exports the function called at that spot (inserted after the
last `loadmodule`, any load form), and **Create route[x]** for an
undefined `route(x)` target (a stub with an `exit;` body is appended
— empty route bodies are not valid Kamailio).

#### Refactorings

**Extract into a route.** Select statements inside a route body and
the lightbulb offers to lift them into a `route[EXTRACTED]` of their
own, leaving a `route(EXTRACTED);` call at the original indentation.
The new block lands after the enclosing one, and the generated name
steps aside from any name already in the file.

The action appears for a selection of whole lines, not for a bare
cursor or a word inside a line — it lifts *lines*, so offering it for
a sub-line selection would move more than was highlighted.

It declines more than it accepts, and each refusal is a case where
accepting would change what the config does:

- a selection outside a route body, or covering the block's own
  braces — there is nothing to lift, or lifting it would unbalance
  the file;
- unbalanced braces inside the selection, for the same reason;
- **a `return` in the selection.** `return` leaves the route it is
  written in. Moved into a new route it returns to the *caller*, so
  the statements after the extracted call would start running when
  they did not before. That is a behaviour change no editor should
  make silently, so the action is simply not offered.

Braces and `return` are judged in code position, so a `return` inside
a string or a comment does not block anything.

**Remove duplicate `loadmodule` lines.** A second `loadmodule` for the
same module is not untidiness — the real parser rejects it outright,
positioned on the second line. The action removes every occurrence
after the first and **does not reorder anything**: load order decides
module initialisation order and a `modparam` must follow its own
`loadmodule`, so sorting is not a transformation that can be applied
blind.

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

Both `textDocument/semanticTokens/full` and **`/range`** are served:
editors that request only the visible slice of a large config get
exactly the tokens whose start position falls inside the range,
delta-encoded from a fresh document-absolute origin per the LSP
spec. (Token deltas/`resultId` are deliberately not implemented —
configs are small enough that full/range recomputation wins.)

#### Formatting

**Shift+Alt+F** (or format-on-save) re-indents the document by brace
depth and strips trailing whitespace. Selecting a region and using
range formatting does the same for those lines only, at the
indentation a whole-document pass would have given them.

The formatter is deliberately **line-preserving**: it rewrites the
leading and trailing whitespace of a line and nothing else. It never
joins, splits or reorders lines, never touches a byte inside a string
literal or a comment body (either comment style), and never emits an
edit for a line that is already correct — so folding, selection and
cursor position survive. Braces inside strings and comments do not
move the indent depth.

Four things it will not touch:

- **Continuation lines of a multi-line string or block comment** —
  their leading whitespace is content, not layout.
- **`#!` and `!!` preprocessor directives** — Kamailio's preprocessor
  reads these line-wise ahead of the parser, so their column is not
  the formatter's to move.
- **Backslash-continued directives** — a `#!define NAME value \`
  spanning lines carries its own layout with it.
- **Lines that continue the previous statement.** Brace depth is not
  the whole story about indentation here. A call whose arguments span
  lines, a condition broken across lines, and the body of a braceless
  `if` are all placed by the author to show what they belong to, and
  none of that shows up in the brace count. A line is only re-indented
  when the previous code line actually *ended* a statement — with `;`,
  `{` or `}`. Dedenting a braceless `if` body would be the worst of
  it: the parse would not change, but the body would read as though it
  runs unconditionally.

Indentation follows the editor: the `insertSpaces` and `tabSize` the
client sends with the request decide tabs versus spaces and the width.
Upstream `src/etc/kamailio.cfg` is tab-indented, which is what a
client sending no preference gets.

The guarantee is tested three ways: the reformatted document must be
identical to the original once leading and trailing whitespace is
stripped from every line; formatting must be idempotent; and, against
a real binary, the positioned parse errors `kamailio -c` reports must
be unchanged by formatting.

#### Pull diagnostics

`textDocument/diagnostic` answers for one document; `workspace/diagnostic`
sweeps the workspace without opening anything. A report carries a
result id, so asking again for something that has not moved comes back
`unchanged` instead of resending the same list.

**Only root configs are reported by the workspace sweep.** A config
that another config includes is a fragment, not a program: checked on
its own it would flag every route its parent defines as undefined and
every construct it continues as a syntax error. Roots are the files
nothing else includes, and their closures already cover the fragments,
so nothing is lost by leaving fragments out — and a great deal of
noise is avoided. The sweep is bounded at 500 configs and says so in
the log when it stops early; a truncated sweep that looks complete
would be worse than one that admits it.

**Pushing stops when the client pulls.** The two are separate channels
and a client that does both shows every problem twice, so the server
picks one based on what the client declared. Because the `-c` check is
asynchronous, a pulling client is answered from the previous checker
result and then sent `workspace/diagnostic/refresh` when the new one
lands — an invitation to ask again, which is how the protocol expects
an async server to behave.

#### Watched files

Three things the server derives answers from can change without ever
arriving as a document edit: a config included by an open file, the
module documentation tree, and the wiki checkout the core docs come
from. A git checkout, a rebuild, or another tool editing an include
all leave the server answering from a stale read until the buffer
happens to be touched.

The server registers for `workspace/didChangeWatchedFiles` on all
three, and reacts to each:

- **An include changed** — every open document whose include closure
  contains that file is re-checked and republished.
- **The tree or wiki changed** — the catalogue is re-harvested. The
  cache fingerprint is content-aware, so a changed file misses the
  cache by construction rather than by special casing.

A re-check driven by a watched file publishes even when the result is
clean. That is deliberate and differs from opening a file: if the
warning on screen is no longer true, saying nothing would leave it
there.

Registration is dynamic, so it only happens when the client declares
support, and the request is time-bounded — a client that declares
support and then never answers cannot stall startup. Tree and wiki
usually live outside the workspace, so their watchers are relative
patterns rooted at each.

#### Documentation before you configure anything

Two catalogues — the core language and every documented module — are
built in and used when nothing is configured:

- **the core language** — parameters, functions and pseudo-variables
  like `debug`, `log_facility`, `children`, harvested from the
  Kamailio 6.1.x cookbook;
- **every documented module** — 254 of them with their exported
  functions and parameters, harvested from the 6.1.4 source tree, so
  `loadmodule "` offers real names and a call like `is_method`
  completes and hovers.

Both are clearly labelled: hover any built-in entry and it says which
version the documentation came from and that setting `kamailioSrc` (or
`kamailioWiki` for the core half) gives you docs exact for your own
build. A configured source **replaces** the matching built-in
catalogue rather than merging with it — blending two versions would be
wrong in a way neither is on its own.

Shipping the module half reverses an earlier decision, which said
there was no honest version to pin module docs to because what modules
exist depends on what you built. That objection was right about the
risk and wrong about the remedy: it applies equally to core
parameters, which move between releases too, and the answer in both
cases is provenance plus a total override rather than silence. Two
things keep it honest:

- **the loaded-module rule still holds** — a module's functions are
  offered only inside a config that `loadmodule`s it, and a core
  global outranks a same-named parameter of a module the config never
  loads;
- **the checker has the last word** — `-c` loads the modules a config
  references, so a module you have not built is reported as a
  diagnostic on the `loadmodule` line itself.

What the built-ins cannot tell you is whether a module is installed on
*your* system: the name list is what 6.1.4 documents, not what you
compiled.


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
| `kamailioLsp.kamailioVersion` | `kamailioVersion` | `KAMAILIO_LSP_VERSION` | *(newest)* | Built-in release to check `modparam` names against. Ignored when `kamailioSrc` is set. |
| `kamailioLsp.versionInHints` | `versionInHints` | `KAMAILIO_LSP_VERSION_IN_HINTS` | `false` | Repeat the release under every hover and completion item. |
| `kamailioLsp.assistance` | `assistance` | `KAMAILIO_LSP_ASSISTANCE` | `true` | Answer hovers and completion. Toggle with <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd>; diagnostics are unaffected. |
| `kamailioLsp.kamailioWiki` | `kamailioWiki` | `KAMAILIO_LSP_WIKI` | *(unset)* | kamailio-wiki checkout for core-language docs. |
| `kamailioLsp.modulesPath` | `modulesPath` | — | *(unset)* | Module search path for the checker (`-L`). |
| `kamailioLsp.diagnostics.enable` | *(maps to empty `kamailioPath`)* | — | `true` | Toggle diagnostics without losing the configured path. |
| `kamailioLsp.diagnostics.analyzer` | `analyzerDiagnostics` | — | `true` | Fast analyzer warnings between saves (undefined `route()` targets, duplicate definitions, undocumented modparams). |
| `kamailioLsp.codeLens.references` | `codeLensReferences` | — | `true` | Reference-count code lenses on route definitions. |
| `kamailioLsp.inlayHints.parameterNames` | `inlayHintParameterNames` | — | `true` | Draw parameter names at documented call sites. |
| `kamailioLsp.inlayHints.defineValues` | `inlayHintDefineValues` | — | `true` | Draw what each preprocessor symbol expands to, at its uses. |
| `kamailioLsp.diagnostics.maxProblems` | `maxDiagnostics` | — | `100` | Bound on published diagnostics per file. |
| `kamailioLsp.checkTimeoutMs` | `checkTimeoutMs` | `KAMAILIO_LSP_CHECK_TIMEOUT_MS` | `10000` | Kill a `-c` run after this many ms. |
| `kamailioLsp.completion.snippets` | `snippetCompletions` | — | `true` | Function completions as tabstop snippets. |
| `kamailioLsp.cacheDir` | `cacheDir` | `KAMAILIO_LSP_CACHE_DIR` | platform cache dir | Documentation-catalog cache location. |
| `kamailioLsp.associateIncludedFiles` | — | — | `true` | Give a plain-text file the workspace's configuration includes the Kamailio language (colours, completion, diagnostics), whatever it is named. Files another extension already claims are left alone. |
| `kamailioLsp.trace.server` | — | — | `off` | LSP traffic tracing in the output channel. |
| — | — | `KAMAILIO_LSP_OUTPUT_CAP_BYTES` | `1048576` | Byte cap on captured `-c` output. |
| — | — | `KAMAILIO_LSP_TRACE_INDEX` | *(unset)* | Set to any non-empty value to log one stderr line per document-index rebuild — a debugging seam for cache behaviour. |

## Notes

- The extension claims `kamailio.cfg`, `kamailio*.cfg` and
  `*.kamailio.cfg` by name, and any file whose FIRST LINE is a
  Kamailio script-type marker — `#!KAMAILIO`, `#!OPENSER`, `#!SER`,
  `#!MAXCOMPAT` or `#!ALL`, the set `src/core/cfg.lex` accepts —
  whatever that file is called. The generic `.cfg` extension is
  deliberately left alone so unrelated tools' config files are not
  hijacked. A file your configuration *includes* is picked up anyway,
  at runtime, whatever it is named: VS Code hands it over as plain
  text, the extension asks the server whether anything includes it
  (`kamailio/analysisRoot`) and sets the language when something
  does, so `carrier-routes.cfg` and `include/globals.inc` both get
  the same colours and the same server as the root that pulls them
  in. A file another extension has already claimed is left to that
  extension.
  Turn this off with `kamailioLsp.associateIncludedFiles`; anything
  the server cannot reach through an include still needs a
  `files.associations` entry mapping it to `kamailio-cfg`.
- The analyzer expands `route(NAME)` through the `#!define` table
  before deciding anything, so a route addressed through an alias
  (`#!define RELAY MYROUTE` + `route(RELAY)`) is resolved rather than
  flagged. Defines from included files count. If the expansion names
  no route either, the warning says so and names both — the alias and
  what it expands to.
- Include handling is capped for safety: depth 8, 64 files, 1 MiB per
  file; relative paths resolve against the including file's
  directory. `KAMAILIO_LSP_ANALYZER_DEBOUNCE_MS` tunes the analyzer
  debounce (default 300).
- Runtime toggles (`diagnostics.analyzer`, `completion.snippets`,
  `codeLens.references`, `inlayHints.parameterNames`,
  `inlayHints.defineValues`, `diagnostics.maxProblems`,
  `checkTimeoutMs`) apply **live**: the VS Code client pushes them to
  the running server via `workspace/didChangeConfiguration` and open
  documents republish immediately. Settings that shape
  initialization (`serverPath`, `kamailioPath`, `kamailioSrc`,
  `kamailioWiki`, `modulesPath`, `cacheDir`, `enable`,
  `diagnostics.enable`) still restart the server automatically.
- Snippet completions and static snippets compose: static snippets
  scaffold blocks, completion snippets fill in calls.
