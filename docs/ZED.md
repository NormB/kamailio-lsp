# Zed setup

Zed cannot be pointed at a language server from its settings file. Its
own documentation is explicit: *"only language server, context server
and debugger extensions require the presence of custom Rust"*. So using
this server from Zed means building a small extension — about forty
lines, most of it copy-paste, and Zed compiles it for you.

This page is the whole procedure. If you just want the reference
version, [`docs/EDITORS.md`](EDITORS.md) has the four files without the
commentary.

## Before you start

You need three things:

| | |
|---|---|
| **Zed** | any recent release |
| **Rust** | via [rustup](https://rustup.rs) — Zed builds the extension with it |
| **`kamailio-lsp` on your PATH** | from the [releases page](https://github.com/NormB/kamailio-lsp/releases), or `cargo build --release` |

Check the last one before going further, because it is the failure
people hit at the end rather than the start:

```sh
kamailio-lsp check --help
```

If that prints a usage line, you are ready.

## 1. Build the extension

Zed clones the tree-sitter grammar itself — `[grammars]` takes a
repository, a revision, and a `path` for a grammar that lives in a
subdirectory, which is this one. Paste the whole block:

```sh
set -e
ROOT="$HOME/.local/share/kamailio-zed"
rm -rf "$ROOT" && mkdir -p "$ROOT"

# the revision Zed should build the grammar from
REV=$(git ls-remote https://github.com/NormB/kamailio-lsp.git HEAD | cut -f1)

# the extension itself
EXT="$ROOT/extension"
mkdir -p "$EXT/src" "$EXT/languages/kamailio-cfg"

cat > "$EXT/extension.toml" <<TOML
id = "kamailio"
name = "Kamailio"
version = "0.0.1"
schema_version = 1
authors = ["you <you@example.com>"]
description = "Kamailio configuration language and LSP"
repository = "https://github.com/NormB/kamailio-lsp"

[grammars.kamailio]
repository = "https://github.com/NormB/kamailio-lsp"
rev = "$REV"
path = "tree-sitter-kamailio"

[language_servers.kamailio-lsp]
name = "kamailio-lsp"
languages = ["Kamailio"]
TOML

cat > "$EXT/languages/kamailio-cfg/config.toml" <<'TOML'
name = "Kamailio"
grammar = "kamailio"
path_suffixes = ["kamailio.cfg"]
first_line_pattern = "^#!(KAMAILIO|OPENSER|SER|MAXCOMPAT|ALL)\\b"
line_comments = ["# "]
TOML

cat > "$EXT/Cargo.toml" <<'TOML'
[package]
name = "zed-kamailio"
version = "0.0.1"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
zed_extension_api = "0.7"
TOML

cat > "$EXT/src/lib.rs" <<'RUST'
use zed_extension_api as zed;

struct KamailioExtension;

impl zed::Extension for KamailioExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let path = worktree
            .which("kamailio-lsp")
            .ok_or_else(|| "kamailio-lsp is not on $PATH".to_string())?;
        Ok(zed::Command {
            command: path,
            args: vec![],
            env: worktree.shell_env(),
        })
    }
}

zed::register_extension!(KamailioExtension);
RUST

echo "extension ready: $EXT"
```

## 2. Install it in Zed

1. Open the **Extensions** page.
2. Run **`zed: install dev extension`** from the command palette.
3. Choose the directory the script printed —
   `~/.local/share/kamailio-zed/extension`.

Zed compiles it at this point. If you already have a published
Kamailio extension installed, Zed removes it first.

## 3. Check it worked

Open any file named `kamailio.cfg` and type `log_`. You should get a
list including `log_level`, `log_prefix` and `log_stdout` — that
documentation is built into the server, so it appears without any
further configuration.

Hover `log_level` and the popup should say it is a core parameter, and
name the Kamailio version the documentation came from.

## 4. Optional settings

Everything below is optional. Put it in Zed's `settings.json`:

```json
{
  "file_types": {
    "Kamailio": ["kamailio*.cfg", "**/*.kamailio.cfg"]
  },
  "lsp": {
    "kamailio-lsp": {
      "initialization_options": {
        "kamailioPath": "/usr/local/sbin/kamailio",
        "kamailioSrc": "/path/to/kamailio"
      }
    }
  }
}
```

- **`file_types`** widens which files count as Kamailio configs. The
  extension's own `path_suffixes` are suffixes rather than globs, so
  they match `kamailio.cfg` and `db.kamailio.cfg` but not
  `kamailio-proxy.cfg`; this setting takes globs and covers the rest.
  The `first_line_pattern` in the language config covers the other
  half: a config opening with `#!KAMAILIO` (or `#!OPENSER`, `#!SER`,
  `#!MAXCOMPAT`, `#!ALL`) is recognised whatever it is named.
- **`kamailioPath`** turns on real diagnostics: the server runs your
  Kamailio binary with `-c` and reports what the parser says.
- **`kamailioSrc`** points at a source tree matching your build, and
  **`kamailioWiki`** at a
  [kamailio-wiki](https://github.com/kamailio/kamailio-wiki) checkout.
  The core language (from the 6.1.x cookbook) and every documented
  module (from the 6.1.4 tree) are already built in; set these when you
  want documentation exact to your own version instead. Each replaces
  the matching built-in catalogue rather than merging with it.

To run a server from somewhere other than `$PATH`:

```json
"lsp": { "kamailio-lsp": { "binary": { "path": "/opt/bin/kamailio-lsp" } } }
```

## When it does not work

| What you see | What it means |
|---|---|
| The extension fails to build | Rust is missing or too old. `rustup update`, then reinstall the dev extension. |
| No syntax colours, no completion | The file name does not match. Add the `file_types` block above, or rename to `kamailio.cfg`. |
| Colours but no completion | The server was not found. `kamailio-lsp` must be on the PATH Zed inherits — check with `zed: open log`, or set `lsp.kamailio-lsp.binary.path`. |
| Nothing at all, no error | Run `zed --foreground` from a terminal; extension load failures appear there. |
| Completion works, no red squiggles | Diagnostics need a real binary: set `kamailioPath`. |

## What this page does not promise

Every build checks that the shell block above is valid shell, that the
Rust in it is byte-identical to the copy in
[`docs/EDITORS.md`](EDITORS.md), and that both pin the same
`zed_extension_api` version — so the two pages cannot drift apart and
the block cannot rot into something that will not parse. When this
page last changed, the block was also run as written and the extension
it produced was compiled for `wasm32-wasip1`.

Zed itself is not part of any of that. Nobody has verified the
end-to-end result inside a running Zed, so if something here is wrong,
please [open an issue](https://github.com/NormB/kamailio-lsp/issues)
and it will be corrected rather than left to rot.
