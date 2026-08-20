# Kamailio LSP Server

## Admin Guide

### Overview

`kamailio-lsp` is a Language Server Protocol server for the Kamailio
configuration file language (`kamailio.cfg`). It provides
diagnostics, context-sensitive completion (modules, parameters,
exported and core functions, pseudo-variables after `$`), hover
documentation, go-to-definition for routes, and document symbols to
any LSP-capable editor. Positions are exchanged in UTF-16 units per
the LSP default, correct on multibyte lines.

Semantic validation is delegated to Kamailio itself: the server runs
`kamailio -c --all-errors -Y <tmpdir> -f <file>` and maps the
parser's own errors (file, line, column — including column ranges and
multi-line spans) to LSP diagnostics, so results are exact for the
Kamailio version installed. Editor intelligence (completion, hover)
comes from a documentation catalog harvested right after
initialization (a readiness log message reports the counts; results
are cached — see Caching): module documentation from the generated
plain-text `README` of every module in a Kamailio source tree
(`src/modules/<name>/README`), and core-language documentation
(parameters, functions, pseudo-variables) from a
[kamailio-wiki](https://github.com/kamailio/kamailio-wiki) checkout
(`docs/cookbooks/<version>/{core,pseudovariables}.md` — the newest
stable cookbook is picked automatically). Version-proven: Kamailio
6.0.x (binary 6.0.1, branch 6.0 tree).

### Dependencies

#### External Libraries or Applications

The server itself has no runtime library dependencies. Optional but
recommended:

* a `kamailio` binary — enables diagnostics (`-c`). Without it (or
  with the parameter set empty) diagnostics are disabled while all
  other features keep working.
* a Kamailio source tree — enables module completion and hover
  documentation.
* a kamailio-wiki checkout — enables core-language completion and
  hover (core parameters, core functions, pseudo-variables).

### Exported Parameters

The parameters below are passed as LSP `initializationOptions` (see
the editor guides in `docs/EDITORS.md`); most have an environment
fallback for clients that cannot pass options.

#### kamailioPath (string)

Path to the `kamailio` binary used for `-c` diagnostics. Set to the
**empty string** to disable diagnostics entirely — see the Security
note below.

*Default value is `kamailio` (PATH lookup). Environment fallback:
`KAMAILIO_LSP_BIN`.*

```json title="Set kamailioPath parameter"
{ "kamailioPath": "/usr/sbin/kamailio" }
```

#### kamailioSrc (string)

Kamailio source tree to harvest module documentation from
(`src/modules/<name>/README`). When unset, completion is limited to
core keywords and route names, and module hover is unavailable.

*Default value is unset. Environment fallback: `KAMAILIO_LSP_SRC`.*

```json title="Set kamailioSrc parameter"
{ "kamailioSrc": "/home/user/src/kamailio" }
```

#### kamailioWiki (string)

kamailio-wiki checkout to harvest core-language documentation from.
Point it at a clone of
`https://github.com/kamailio/kamailio-wiki` (the newest stable
`docs/cookbooks/<N.N.x>` is picked), or directly at a directory
containing `core.md` and `pseudovariables.md`.

*Default value is unset. Environment fallback: `KAMAILIO_LSP_WIKI`.*

```json title="Set kamailioWiki parameter"
{ "kamailioWiki": "/home/user/src/kamailio-wiki" }
```

#### modulesPath (string)

Module search path passed to the checker as `-L <path>`. Useful when
the modules matching your configuration are not in the binary's
default `mpath`.

*Default value is unset (the binary's compiled-in default applies).*

```json title="Set modulesPath parameter"
{ "modulesPath": "/usr/lib/x86_64-linux-gnu/kamailio/modules" }
```

#### checkTimeoutMs (integer)

Upper bound, in milliseconds, on one `kamailio -c` run. A run that
exceeds it is killed and reported via a client log message.

*Default value is `10000`. Environment fallback:
`KAMAILIO_LSP_CHECK_TIMEOUT_MS`.*

```json title="Set checkTimeoutMs parameter"
{ "checkTimeoutMs": 3000 }
```

### Caching

Harvest results are cached per (source tree, wiki) pair under
`$XDG_CACHE_HOME/kamailio-lsp` (or `~/.cache/kamailio-lsp`), keyed by
a fingerprint of the canonical paths and the modification times of
the tree's `src/modules/` directory and the wiki's cookbook directory
— adding or removing a module or cookbook page invalidates the cache
automatically. The readiness log message says `, cached` on a hit.
Override the location with the `KAMAILIO_LSP_CACHE_DIR` environment
variable (env-only knob); delete the directory to force a re-harvest.

#### maxDiagnostics (integer)

Bound on the diagnostics published per file.

*Default value is `100`.*

```json title="Set maxDiagnostics parameter"
{ "maxDiagnostics": 50 }
```

#### analyzerDiagnostics (boolean)

Fast analyzer warnings between saves, debounced as you type:
`route(NAME)` calls whose target is defined nowhere in the file or
its `include_file`/`import_file` closure, and duplicate route
definitions. Severity warning, source `kamailio-lsp`; merged with the
stored `kamailio -c` results on every publish. The debounce is
tunable via `KAMAILIO_LSP_ANALYZER_DEBOUNCE_MS` (default 300).

*Default value is `true`.*

```json title="Set analyzerDiagnostics parameter"
{ "analyzerDiagnostics": false }
```

#### codeLensReferences (boolean)

Show a reference-count code lens above every named `route` block
(only main-table blocks are `route()`-callable, so only they get a
count; references are counted across the include closure).

*Default value is `true`.*

```json title="Set codeLensReferences parameter"
{ "codeLensReferences": false }
```

#### snippetCompletions (boolean)

Insert function completions as tabstop snippets.

*Default value is `true`.*

```json title="Set snippetCompletions parameter"
{ "snippetCompletions": false }
```

#### cacheDir (string)

Documentation-catalog cache directory.

*Default value is the platform cache dir. Environment fallback:
`KAMAILIO_LSP_CACHE_DIR`.*

```json title="Set cacheDir parameter"
{ "cacheDir": "/var/cache/kamailio-lsp" }
```

### Security

`kamailio -c` **dlopens the modules the configuration loads**, so
their constructors run: opening a configuration from an untrusted
source executes code. Rely on your editor's workspace-trust
mechanism, or set `kamailioPath` to the empty string for untrusted
trees. `-c` runs are serialized (one at a time), bounded by
`checkTimeoutMs`, and their output is byte-capped
(`KAMAILIO_LSP_OUTPUT_CAP_BYTES`, default 1 MiB).

### Frequently Asked Questions

#### Why do I see no diagnostics?

Either `kamailioPath` is empty/unresolvable (check the editor's LSP
log for the startup warning), or the file is not saved to disk —
diagnostics run against the on-disk file on open and save.

#### Why does completion show no module functions?

`kamailioSrc` is not set, or the module is not `loadmodule`-ed in the
current file: function completion is intentionally limited to loaded
modules. Core functions, parameters, and pseudo-variables come from
the wiki checkout (`kamailioWiki`).

#### Completion looks stale after I updated the source tree

Editing a file inside a module does not bump the directory mtimes the
cache fingerprint watches. Delete the cache directory (see Caching)
or touch `src/modules/` to force a re-harvest.

#### The checker complains it cannot find modules

The `-c` run resolves `loadmodule` against the binary's compiled-in
module path. Point `modulesPath` at the directory holding the `.so`
files that match your configuration (it becomes `-L <path>`).

### License

Dual-licensed under MIT or Apache-2.0, at your option. See
`LICENSE-MIT` and `LICENSE-APACHE` in the repository root.
