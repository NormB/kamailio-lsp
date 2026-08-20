# Editor setup

Install the server first — either grab a prebuilt binary from the
[releases page](https://github.com/NormB/kamailio-lsp/releases)
(x86_64/aarch64 Linux tarballs), or `cargo build --release` and put
`target/release/kamailio-lsp` on PATH.
All examples pass the same three `initializationOptions` documented
in `docs/ADMIN.md`.

## VS Code

**Novice?** Use the [Getting Started guide](GETTING_STARTED.md)
instead — one-command install and full usage walkthrough. The notes
below are for building the extension from source.

Use the bundled extension in `client/`:

```sh
cd client && npm install && npm run compile
npx @vscode/vsce package        # produces kamailio-lsp-ext-<version>.vsix
code --install-extension kamailio-lsp-ext-*.vsix
```

Settings: `kamailioLsp.serverPath`, `kamailioLsp.kamailioPath`,
`kamailioLsp.kamailioSrc`, `kamailioLsp.kamailioWiki`,
`kamailioLsp.checkTimeoutMs`.

## Neovim (0.10+, built-in LSP)

```lua
vim.filetype.add({ filename = { ["kamailio.cfg"] = "kamailio-cfg" } })
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
        checkTimeoutMs = 10000,
      },
    })
  end,
})
```

## Helix

`~/.config/helix/languages.toml`:

```toml
[language-server.kamailio-lsp]
command = "kamailio-lsp"

[language-server.kamailio-lsp.config]
kamailioPath = "/usr/sbin/kamailio"
kamailioSrc = "/path/to/kamailio"
kamailioWiki = "/path/to/kamailio-wiki"

[[language]]
name = "kamailio-cfg"
scope = "source.kamailio"
file-types = [{ glob = "kamailio.cfg" }]
comment-token = "#"
language-servers = ["kamailio-lsp"]
```

## Emacs (eglot, built-in since 29)

```elisp
(define-derived-mode kamailio-cfg-mode prog-mode "Kamailio-cfg"
  (setq-local comment-start "# "))
(add-to-list 'auto-mode-alist '("kamailio\\.cfg\\'" . kamailio-cfg-mode))
(with-eval-after-load 'eglot
  (add-to-list 'eglot-server-programs
               `(kamailio-cfg-mode
                 . ("kamailio-lsp"
                    :initializationOptions
                    (:kamailioPath "/usr/sbin/kamailio"
                     :kamailioSrc "/path/to/kamailio"
                     :kamailioWiki "/path/to/kamailio-wiki"
                     :checkTimeoutMs 10000)))))
```

## Vim (prabirshrestha/vim-lsp)

```vim
if executable('kamailio-lsp')
  au User lsp_setup call lsp#register_server({
    \ 'name': 'kamailio-lsp',
    \ 'cmd': {server_info->['kamailio-lsp']},
    \ 'initialization_options': {
    \   'kamailioPath': '/usr/sbin/kamailio',
    \   'kamailioSrc': '/path/to/kamailio',
    \   'kamailioWiki': '/path/to/kamailio-wiki'},
    \ 'allowlist': ['kamailio-cfg'],
    \ })
endif
au BufRead,BufNewFile kamailio.cfg setfiletype kamailio-cfg
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
        "kamailioPath": "/usr/sbin/kamailio",
        "kamailioSrc": "/path/to/kamailio",
        "kamailioWiki": "/path/to/kamailio-wiki"
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
        "kamailioPath": "/usr/sbin/kamailio",
        "kamailioSrc": "/path/to/kamailio",
        "kamailioWiki": "/path/to/kamailio-wiki"
      }
    }
  }
}
```

## Environment-variable fallback (any client)

Clients that cannot pass `initializationOptions` can export:

```sh
export KAMAILIO_LSP_BIN=/usr/sbin/kamailio
export KAMAILIO_LSP_SRC=/path/to/kamailio
export KAMAILIO_LSP_WIKI=/path/to/kamailio-wiki
export KAMAILIO_LSP_CHECK_TIMEOUT_MS=10000
```
