# Getting Started

This guide assumes no prior experience — just VS Code installed and
an `kamailio.cfg` file you want to edit.

## Install

Works the same on **Linux, macOS, and Windows** — every release ships
native builds for all three (x86_64 and arm64), and the platform
extension packages bundle the server, so your editor picks the right
one automatically.

### Option A — from your editor's marketplace

**VSCodium / Cursor / Gitpod** (and other Open VSX editors): press
**Ctrl+Shift+X**, search for **kamailio**, click **Install** on
"Kamailio Routing Script" — done; the platform builds bundle
everything.

**Standard VS Code** ships with Microsoft's marketplace, where this
extension is not distributed — use Option B (one command, installs
the extension for you) or Option C.

### Option B — one command in a terminal

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/NormB/kamailio-lsp/main/install.ps1 | iex
```

**Linux / macOS:**

Open a terminal (in VS Code: **Terminal → New Terminal**), paste this
line, and press Enter:

```sh
curl -fsSL https://raw.githubusercontent.com/NormB/kamailio-lsp/main/install.sh | sh
```

That's it. The script downloads the right build for your machine,
installs the server to `~/.local/bin`, and adds the extension to VS
Code. It prints what it did; if something is missing (for example the
`code` command), it prints exactly what to do instead.

### Option C — by hand, step by step

1. Open <https://github.com/NormB/kamailio-lsp/releases/latest> in a
   browser.
2. Download two files from the **Assets** list:
   - `kamailio-lsp-…-x86_64-linux-gnu.tar.gz` (or `aarch64` on ARM)
   - `kamailio-lsp-ext-….vsix`
3. Install the server — Linux/macOS in a terminal (Windows: just
   unzip `kamailio-lsp-…-windows.zip` anywhere, e.g.
   `%LOCALAPPDATA%\kamailio-lsp`):

   ```sh
   tar xzf kamailio-lsp-*-linux-gnu.tar.gz    # or *-darwin.tar.gz
   mkdir -p ~/.local/bin
   install -m755 kamailio-lsp ~/.local/bin/
   ```

4. Install the extension — in VS Code:
   1. Press **Ctrl+Shift+X** (the Extensions panel opens).
   2. Click the **⋯** button in the panel's top-right corner.
   3. Choose **Install from VSIX…**
   4. Pick the `kamailio-lsp-ext-….vsix` file you downloaded.

## First use

Open a folder containing an `kamailio.cfg` (**File → Open Folder…**)
and click the file. You should immediately see **syntax colors**.
If VS Code asks *"Do you trust the authors of the files in this
folder?"* — answer honestly: in an untrusted folder the extension
still colors, completes, and navigates, but it will not run the
Kamailio checker on the file (that is a safety feature, because
checking a config executes parts of it).

### See your mistakes as you type (diagnostics)

This needs Kamailio itself installed on the same machine.

1. Press **Ctrl+,** (Settings), type `kamailio` in the search box.
2. In **Kamailio Lsp: Kamailio Path** enter the full path of your
   `kamailio` binary, e.g. `/usr/sbin/kamailio`.
3. Open your `kamailio.cfg` and save it (**Ctrl+S**).

Mistakes now get **red squiggles** at the exact spot — hover one to
read the message (it is the real Kamailio parser talking, e.g.
`parameter <fr_timeot> of type <2:int> not found in module <tm>`). Squiggles refresh
every time you save.

### Autocomplete

- Type `loadmodule "` — a list of every module appears. Keep typing
  to filter, press **Enter** to accept.
- Type `modparam("tm", "` — the list shows only tm's parameters,
  each with its documentation.
- Inside a route, type the first letters of a function
  (`t_re…` → `t_relay`) — functions of the modules you loaded, plus
  core functions, appear with their signatures.
- Type `$` — pseudo-variables (`$ru`, `$si`, …) with descriptions.
- If a list ever disappears, press **Ctrl+Space** to bring it back.

For the richest documentation in these popups, also set
**Kamailio Lsp: Kamailio Src** (in the same Settings page) to a
folder containing the Kamailio source code matching your version, and
**Kamailio Lsp: Kamailio Wiki** to a clone of the
[kamailio-wiki](https://github.com/kamailio/kamailio-wiki) repository
(core parameters, functions, and pseudo-variables).

### Reading and moving around

- **Hover** the mouse over any function, parameter, or `$variable`
  to read what it does.
- **Ctrl+Click** on a route name inside `route(name)` to jump to
  where that route is defined.
- Press **Ctrl+Shift+O** to see every route in the file and jump
  between them.

## When something doesn't work

| Symptom | Fix |
|---|---|
| No colors | The file must be named `kamailio.cfg` (or start with a `#!KAMAILIO` first line). |
| No red squiggles | Set **Kamailio Path** (step above), save the file, and make sure you trusted the folder. |
| Squiggles on a correct file | The checker uses *your* Kamailio version — a config written for another version can legitimately fail. |
| Completion has no documentation | Set **Kamailio Src** to a Kamailio source folder. |
| Still stuck | **View → Output**, pick **Kamailio LSP** in the dropdown — the server explains what it is doing (e.g. "ready (193 documented modules)"). |
