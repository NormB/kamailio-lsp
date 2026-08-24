# Editor and tool setup

The server speaks LSP 3.17 over stdio and knows nothing about any
particular editor, so **any LSP client can drive it**. The sections
below are worked examples for the clients people actually use; if
yours is not here, the shape is always the same — run `kamailio-lsp`,
speak LSP on its stdin/stdout, and pass the settings either as
`initializationOptions` or as environment variables.

Install the server first — either grab a prebuilt binary from the
[releases page](https://github.com/NormB/kamailio-lsp/releases)
(Linux/macOS tarballs and Windows zips, x86_64 and aarch64/arm64), or
`cargo build --release` and put `target/release/kamailio-lsp` on PATH.

**You do not need a source tree to get started.** The core language
(from the 6.1.x cookbook) and all 254 documented modules (from the
6.1.4 tree) are built in, so completion and hover work immediately.
Set `kamailioSrc` for module docs exact to your own build, and
`kamailioWiki` for the core half — each replaces the matching built-in
catalogue wholesale. Every setting is optional; the full
list is in [`docs/FEATURES.md`](FEATURES.md).

## Which files the server should be given

The VS Code extension claims `kamailio.cfg`, `kamailio*.cfg` (so
`kamailio-proxy.cfg` works) and `*.kamailio.cfg`, and also honours a
script-type marker on the first line: `#!KAMAILIO`, `#!OPENSER`,
`#!SER`, `#!MAXCOMPAT` or `#!ALL`. It deliberately does NOT claim
every `.cfg` on disk — that would hijack unrelated files. Configure
your client to match the same set rather than a bare `.cfg` glob.

## VS Code / VSCodium

**Novice?** Use the [Getting Started guide](GETTING_STARTED.md)
instead — one-command install and full usage walkthrough. The notes
below are for building the extension from source.

```sh
cd client && npm install && npm run compile
npx @vscode/vsce package        # produces kamailio-lsp-ext-<version>.vsix
code --install-extension kamailio-lsp-ext-*.vsix
```

Settings live under the `kamailioLsp.` prefix — `kamailioLsp.serverPath`,
`kamailioLsp.kamailioPath`, `kamailioLsp.kamailioSrc`,
`kamailioLsp.checkTimeoutMs`, and the rest listed in
[`docs/FEATURES.md`](FEATURES.md).

## Neovim (0.10+, built-in LSP)

```lua
vim.filetype.add({
  filename = { ["kamailio.cfg"] = "kamailio-cfg" },
  pattern = {
    ["kamailio.*%.cfg"] = "kamailio-cfg",
    [".*%.kamailio%.cfg"] = "kamailio-cfg",
  },
})
vim.api.nvim_create_autocmd("FileType", {
  pattern = "kamailio-cfg",
  callback = function()
    vim.lsp.start({
      name = "kamailio-lsp",
      cmd = { "kamailio-lsp" },
      root_dir = vim.fs.dirname(vim.api.nvim_buf_get_name(0)),
      -- every option is optional; omit the table for built-in docs
      init_options = {
        kamailioPath = "/usr/local/sbin/kamailio",
        kamailioSrc = "/path/to/kamailio",
        checkTimeoutMs = 10000,
      },
    })
  end,
})
```

## coc.nvim

`:CocConfig`:

```json
{
  "languageserver": {
    "kamailio-lsp": {
      "command": "kamailio-lsp",
      "filetypes": ["kamailio-cfg"],
      "initializationOptions": {
        "kamailioPath": "/usr/local/sbin/kamailio"
      }
    }
  }
}
```

## Helix

`~/.config/helix/languages.toml`:

```toml
[language-server.kamailio-lsp]
command = "kamailio-lsp"

[language-server.kamailio-lsp.config]
kamailioPath = "/usr/local/sbin/kamailio"

[[language]]
name = "kamailio-cfg"
scope = "source.kamailio"
file-types = [
  { glob = "kamailio.cfg" },
  { glob = "kamailio*.cfg" },
  { glob = "*.kamailio.cfg" },
]
comment-token = "#"
language-servers = ["kamailio-lsp"]
```

Helix can also use the tree-sitter grammar in `tree-sitter-kamailio/`
for highlighting and folding; see the repository README.

## Emacs (eglot, built-in since 29)

```elisp
(define-derived-mode kamailio-cfg-mode prog-mode "Kamailio-cfg"
  (setq-local comment-start "# "))
(add-to-list 'auto-mode-alist '("kamailio[^/]*\\.cfg\\'" . kamailio-cfg-mode))
(add-to-list 'auto-mode-alist '("\\.kamailio\\.cfg\\'" . kamailio-cfg-mode))
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               `(kamailio-cfg-mode
                 . ("kamailio-lsp"
                    :initializationOptions
                    (:kamailioPath "/usr/local/sbin/kamailio"
                     :checkTimeoutMs 10000)))))
```

## Vim (prabirshrestha/vim-lsp)

```vim
if executable('kamailio-lsp')
  au User lsp_setup call lsp#register_server({
    \ 'name': 'kamailio-lsp',
    \ 'cmd': {server_info->['kamailio-lsp']},
    \ 'initialization_options': {
    \   'kamailioPath': '/usr/local/sbin/kamailio'},
    \ 'allowlist': ['kamailio-cfg'],
    \ })
endif
au BufRead,BufNewFile kamailio.cfg,kamailio*.cfg,*.kamailio.cfg setfiletype kamailio-cfg
```

## Sublime Text (LSP package)

`Preferences → Package Settings → LSP → Settings`:

```json
{
  "clients": {
    "kamailio-lsp": {
      "enabled": true,
      "command": ["kamailio-lsp"],
      "selector": "source.kamailio",
      "initializationOptions": {
        "kamailioPath": "/usr/local/sbin/kamailio"
      }
    }
  }
}
```

## Kate

`Settings → Configure Kate → LSP Client → User Server Settings`:

```json
{
  "servers": {
    "kamailio-cfg": {
      "command": ["kamailio-lsp"],
      "highlightingModeRegex": "^INI Files$",
      "initializationOptions": {
        "kamailioPath": "/usr/local/sbin/kamailio"
      }
    }
  }
}
```

## JetBrains IDEs (IntelliJ, PyCharm, …)

JetBrains IDEs do not read a config file for third-party language
servers; install the **LSP4IJ** plugin, then *Settings → Languages &
Frameworks → Language Servers → +* and fill in:

- **Command:** `kamailio-lsp`
- **Mappings → File name patterns:** `kamailio.cfg`, `kamailio*.cfg`,
  `*.kamailio.cfg`
- **Configuration:** the same JSON object the other clients pass as
  `initializationOptions`

## Zed

Zed registers language servers through an extension rather than
through `settings.json`, so pointing it at this binary means writing a
small Zed extension. The tree-sitter grammar in
`tree-sitter-kamailio/` is the starting point for the language half;
the server half is a standard `command` entry. No worked example is
given here because none has been tested against a Zed release — treat
it as unproven rather than supported.

## Any other LSP client

The server reads LSP on stdin and writes it on stdout. Nothing else is
required: no port, no daemon, no configuration file. Clients that
cannot pass `initializationOptions` can use the environment instead:

```sh
export KAMAILIO_LSP_BIN=/usr/local/sbin/kamailio
export KAMAILIO_LSP_SRC=/path/to/kamailio
export KAMAILIO_LSP_CHECK_TIMEOUT_MS=10000
```

## Without an editor: CI, hooks, and scripts

`kamailio-lsp check` runs the same analysis in batch and prints
`file:line:col: severity: message`, which most tooling already parses:

```console
$ kamailio-lsp check /etc/kamailio/kamailio.cfg
/etc/kamailio/kamailio.cfg:2:11: warning: route 'MISSING' is not defined here or in included files
```

Exit codes are 0 clean, 1 problems found (errors, or warnings under
`--strict`), 2 usage or read failure — so it drops straight into a
gate. Usage is
`kamailio-lsp check [--strict] [--bin <kamailio>] <file>...`.

GitHub Actions:

```yaml
- name: Check Kamailio configs
  run: kamailio-lsp check --strict $(git ls-files 'kamailio*.cfg' '*.kamailio.cfg')
```

A pre-commit hook:

```sh
#!/bin/sh
files=$(git diff --cached --name-only --diff-filter=ACM | grep -E '(^|/)kamailio[^/]*\.cfg$|\.kamailio\.cfg$')
[ -z "$files" ] && exit 0
kamailio-lsp check --strict $files
```

Passing `--bin` (or `KAMAILIO_LSP_BIN`) additionally runs the real
Kamailio parser over each file, so CI catches what only the binary
knows. Without it the check is static analysis alone.
