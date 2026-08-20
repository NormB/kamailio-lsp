# kamailio-lsp

A Language Server Protocol implementation for the Kamailio
configuration file language (`kamailio.cfg`).

## What it does

| Feature | How |
|---|---|
| **Diagnostics** | Runs `kamailio -c --all-errors -Y <tmpdir> -f <file>` on open/save and maps the parser's errors (line, column, column ranges, multi-line spans) to LSP diagnostics — full-fidelity, version-exact semantic validation by the real parser. A fast analyzer layer warns between saves (debounced on change): undefined `route()` targets and duplicate route definitions. |
| **Completion** | Context-sensitive: module names after `loadmodule "` / `modparam("`, the module's parameters inside the second `modparam` argument, exported functions of *loaded* modules plus core functions/parameters, route names inside `route(` and in route bodies, keywords, and pseudo-variables after `$` (replacing the typed token). Duplicate labels collapse; `include_file`/`import_file` closures count. |
| **Signature help** | The innermost unclosed call's signature with the active parameter, on `(` and `,`. |
| **Hover** | Documentation for module functions, parameters, and modules, harvested from Kamailio's own docs. |
| **Go to definition** | `route(NAME)` references resolve to their `route[NAME]` block — in this file or any included file. |
| **References / rename / highlights** | Every call site + definition of a route name; rename rewrites them all (charset-gated, quoted call sites handled). |
| **Document symbols** | All route blocks (`request_route`, `route[...]`, `failure_route[...]`, `event_route[...]`, …) with full block extents, nested outline. |
| **Folding** | Route-family blocks fold; brace matching is string/comment-safe. |
| **Workspace symbols / code lenses** | Ctrl+T searches route definitions across open files + includes; named callable routes show closure-wide reference counts. |
| **Quick fixes** | Load the module exporting an unknown command; create a stub for an undefined `route(x)`. |
| **Catalog validation** | `modparam` parameters the configured tree does not document warn as you type. |
| **Semantic tokens** | Route names + pseudo-variables (both string quote styles), UTF-16 delta-encoded. |
| **CLI** | `kamailio-lsp check [--strict] [--bin <kamailio>] <file>...` for CI and git hooks (exit 0/1/2). |

Positions are exchanged in UTF-16 units (the LSP default) and are
correct on multibyte lines; doc harvests are cached per source tree
(see the admin guide's Caching section).

The documentation catalog is harvested at startup from two places:
module docs from the generated plain-text `README` in every
`src/modules/<name>/` directory of a Kamailio source tree, and
core-language docs (parameters, functions, pseudo-variables) from a
[kamailio-wiki](https://github.com/kamailio/kamailio-wiki) checkout
(`docs/cookbooks/<version>/` — the newest stable cookbook is picked).

Supported and version-proven: **Kamailio 6.0.x** — the proof suite
runs against a real tree, wiki, and binary
(`KAMAILIO_LSP_TEST_TREE`/`KAMAILIO_LSP_TEST_WIKI`/`KAMAILIO_LSP_TEST_BIN`).

## Configuration

Via LSP `initializationOptions` (or environment fallback):

| Option | Env | Default | Meaning |
|---|---|---|---|
| `kamailioPath` | `KAMAILIO_LSP_BIN` | `kamailio` | Binary used for `-c` diagnostics. |
| `kamailioSrc` | `KAMAILIO_LSP_SRC` | *(none)* | Source tree to harvest module docs from. |
| `kamailioWiki` | `KAMAILIO_LSP_WIKI` | *(none)* | kamailio-wiki checkout for core-language docs. |
| `modulesPath` | — | *(none)* | Module search path for the checker (`-L`). |

Diagnostics fidelity note: `-c` loads the modules the cfg references,
so it needs an installation where those `.so` files exist (an
unresolvable module is itself reported as a diagnostic, which is
usually what you want; `modulesPath` points the checker elsewhere).

## Install

**New to all of this?** Follow the
[Getting Started guide](docs/GETTING_STARTED.md) — one-command
install plus click-by-click usage instructions. Short version:

```sh
curl -fsSL https://raw.githubusercontent.com/NormB/kamailio-lsp/main/install.sh | sh
```

Prebuilt server binaries (Linux, macOS, and Windows — x86_64 and
arm64) and the VS Code `.vsix` ship with every
[GitHub release](https://github.com/NormB/kamailio-lsp/releases):

```sh
tar xzf kamailio-lsp-<version>-x86_64-linux-gnu.tar.gz
install -m755 kamailio-lsp ~/.local/bin/
```

## Build & test

```sh
cargo build --release        # server binary: target/release/kamailio-lsp
cargo test                   # full suite, includes a stdio LSP e2e test
```

## Tree-sitter grammar

`tree-sitter-kamailio/` carries an error-tolerant grammar for editors
that highlight and fold via tree-sitter (Neovim, Helix, Zed): corpus
tests run in CI; `tree-sitter generate` builds the parser locally.

## Documentation

- [`docs/FEATURES.md`](docs/FEATURES.md) — every feature, every
  setting (VS Code / init option / environment), and the snippet set.
- [`docs/ADMIN.md`](docs/ADMIN.md) — admin guide (overview,
  dependencies, exported parameters, security, FAQ).
- [`docs/EDITORS.md`](docs/EDITORS.md) — setup for VS Code, Neovim,
  Helix, Emacs, Vim, Sublime Text, and Kate.
- API docs: `cargo doc --open` (`missing_docs` is `deny`).

## Editors

- **VS Code**: the `client/` directory contains the extension
  (`npm install && npm run compile`, then run/package with vsce).
  Settings: `kamailioLsp.serverPath`, `kamailioLsp.kamailioPath`,
  `kamailioLsp.kamailioSrc`, `kamailioLsp.kamailioWiki`.
- **Neovim** (0.10+):

  ```lua
  vim.api.nvim_create_autocmd("FileType", {
    pattern = "kamailio-cfg",
    callback = function()
      vim.lsp.start({
        name = "kamailio-lsp",
        cmd = { "kamailio-lsp" },
        init_options = {
          kamailioPath = "/usr/sbin/kamailio",
          kamailioSrc = "/path/to/kamailio",
          kamailioWiki = "/path/to/kamailio-wiki",
        },
      })
    end,
  })
  ```

## Design

- `src/catalog.rs` — module-README + wiki-cookbook documentation
  harvester
- `src/analyze.rs` — comment/string-aware lexical scan of cfg text
  (loadmodules, routes, cursor context); deliberately *not* a grammar
- `src/diag.rs` — `kamailio -c` output parser
- `src/logic.rs` — pure completion/hover/definition assembly
- `src/server.rs` — tower-lsp wiring

Semantic truth stays in Kamailio itself (`-c`); the server never
guesses about grammar validity, so it is automatically correct for
whatever Kamailio version it is pointed at.

## Security note

`kamailio -c` **dlopens the modules the cfg loads** — their
constructors run. Opening a config from an untrusted source therefore
executes code paths you did not write. Rely on your editor's
workspace-trust prompt, and/or disable diagnostics entirely by
setting `kamailioPath` (or `KAMAILIO_LSP_BIN`) to an **empty string**
— completion, hover, and navigation keep working without it.
`-c` runs are serialized and bounded (10s default,
`KAMAILIO_LSP_CHECK_TIMEOUT_MS` to tune).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any
contribution intentionally submitted for inclusion in the work by
you, as defined in the Apache-2.0 license, shall be dual licensed as
above, without any additional terms or conditions.
