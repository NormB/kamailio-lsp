# Changelog

All notable changes to the Kamailio Routing Script extension.

## [0.22.1] — 2026-11-03

### Fixed

- **The formatter removes only what Kamailio reads as whitespace: space,
  tab, backspace and CR.**
  - Was: it removed a form feed too, so `#!endif` followed by a form feed,
    a comment to Kamailio, became a directive.
  - Effect: formatting such a file a second time no longer changes it
    again.

### Security

- **GHSA-g3hv-75wg-m249 in minimatch is recorded as not affecting the
  extension.**
  - Effect: the server registers no file operations and the document
    selector has no pattern, so no user, workspace or file content reaches
    minimatch.
  - Effect: `client/osv-scanner.toml` holds the decision for OSV-Scanner
    and the OpenSSF Scorecard until 2027-02-01.
  - Limit: minimatch 10.2.6 stays; vscode-languageclient 11.0.0 requires
    it, and minimatch 11.0.1 removes `makeRe()`, which the client calls.
  - Docs: [Advisories recorded as not affecting kamailio-lsp](https://github.com/NormB/kamailio-lsp/blob/main/SECURITY.md#advisories-recorded-as-not-affecting-kamailio-lsp).

## [0.22.0] — 2026-10-27

### Added

- **Completion inside `include_file "` and `import_file "` lists
  directories and configuration files.**
  - Effect: `.cfg`, `.inc` and `.m4` files complete from the main config's
    directory, then the including file's; `/` opens the next directory.
  - Was: the list offered keywords and function names there.
  - Limit: Kamailio refuses an include name in single quotes, so
    `include_file '` offers nothing.
- **A warning when the include closure stops at 500 files.**
  - Effect: the analyzed file carries "the include closure stopped at 500
    files" on its first line, in the editor and in `kamailio-lsp check`.
  - Was: a route defined only in a file past the bound was reported as
    undefined, with no reason given.
  - Limit: a configuration of exactly 500 files, or one whose next include
    names a missing file, gets no warning.
  - Docs: [An included file opened on its own](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#an-included-file-opened-on-its-own).
- **Second names the C sources export are listed as aliases.**
  - Effect: an export sharing a documented function's handler and
    parameter count gets that function's documentation with an "Alias of"
    note.
  - Effect: `sl_reply`, `t_disable_e2e_cancel_reason`, `ds_list_exist`,
    `avp_printf`, `set_attr`, `pv_www_authorize`, `pv_proxy_authorize`,
    `www_authenticate` and `proxy_authenticate` are listed.

### Fixed

- **Functions that are not script functions are no longer listed.**
  - Was: completion and hover offered `tm`'s C API (`int t_suspend`,
    `register_tmcb`) and `db2_ops`'s `@db.query...` selects.
  - Was: `textops`' and `xprint`'s Developer Guide functions and
    `app_java`'s "Common requirements" were offered too.
- **Functions that were missing are now listed.**
  - Effect: every function of `auth_web3`, plus
    `rtpengine_subscribe_request` and `crypto_netio_in`, is listed.
  - Was: the 6.1.5 `auth_web3` README is not valid UTF-8 and was skipped
    whole.
- **Formatting leaves a directive line exactly as it stands.**
  - Was: trailing whitespace on `#!` lines was trimmed, though Kamailio
    reads `#!def` followed by a space as a directive and without it as a
    comment.
- **A failed check never shows an empty message.**
  - Was: when `kamailio -c` failed with no positioned error and its last
    error line had no text after the prefix, the diagnostic was empty.
  - Effect: the nearest earlier error line with a reason is used, or a
    fixed message.

## [0.21.6] — 2026-10-27

### Changed

- **The analyzer follows includes 10 levels deep and reads up to 500
  files.**
  - Was: it stopped at 8 levels and 64 files, so a deeper or later route
    was reported as "not defined here or in included files".
  - Effect: the depth is 10 levels below the main config, as in Kamailio
    (`MAX_INCLUDE_DEPTH` in `cfg.lex`).

### Fixed

- **A top-level `name = value` after a blank line is listed once in the
  outline.**
  - Was: it was listed twice, the second entry with a range that ended
    before it started.

## [0.21.5] — 2026-10-26

### Changed

- **The server is built with tokio 1.54.1; the extension uses
  vscode-languageclient 11.0.0.**
  - Effect: the minimum VS Code version is unchanged (1.91).

### Fixed

- **An included file is part of a configuration only when Kamailio reads
  it.**
  - Effect: an `include_file` inside `#!ifdef NAME` counts as read only
    when `NAME` is defined above it.
  - Effect: `#!defexp`, `#!defexps`, `#!defenv`, `#!defenvs`, `#!substdef`
    and `#!substdefs` count as defining the name.
  - Was: a `#!define` in a skipped branch or a `/* */` comment counted, so
    a file was checked through a configuration that never opens it.
- **A `#!ifdef`, `#!else` or `#!endif` inside a comment or string is read
  as text.**
  - Was: a route commented out together with its `#!ifdef` was reported as
    "defined more than once" and appeared in the outline.
  - Limit: this applies to a `/* */` comment or a string outside every
    conditional.

## [0.21.4] — 2026-10-22

### Changed

- **No behavior change: the language server is rebuilt against tokio
  1.53.3.**
  - Effect: tokio moves from 1.53.2 to 1.53.3; server and extension code
    are unchanged from 0.21.3.

## [0.21.3] — 2026-10-19

### Fixed

- **"an `kamailio` binary" reads "a `kamailio` binary" in the extension's
  README.**
  - Effect: no behavior change; server and extension code are unchanged
    from 0.21.2.

## [0.21.2] — 2026-10-19

### Changed

- **The built-in catalog moves to Kamailio 6.1.5 and 6.0.8.**
  - Effect: the `kamailioVersion` choices are `5.8.8`, `6.0.8` and
    `6.1.5`.
  - Effect: `topoh`'s `mask_cookie` parameter is known; on `6.0.8` it is
    reported as existing only in 6.1.5.
  - Limit: a setting of `6.0.7` or `6.1.4` is reported as unrecognized and
    `6.1.5` is used.
  - Docs: [Checking against a specific Kamailio release](https://github.com/NormB/kamailio-lsp/blob/main/docs/GETTING_STARTED.md#checking-against-a-specific-kamailio-release).
- **Core docs come from the current kamailio-wiki 6.1.x cookbook.**
  - Effect: the core parameter `dns_cache_mode`, new in 6.1.5, completes;
    its hover says the cookbook does not describe it yet.
- **The documentation states what the tests prove.**
  - Effect: the real-binary proof runs against Kamailio 6.1.5; 6.0.8 and
    5.8.8 are checked against their source trees.
  - Effect: the readiness message example shows 65 core functions, as the
    server reports.

### Fixed

- **A `//`, `#`, quote or brace inside a `$(...)` pseudo-variable no
  longer breaks the outline.**
  - Was: `$ru = $(ru{re.subst,/;t=[a-z]*//g});` made its route run to the
    end of the file in the outline.
  - Effect: inside `$(...)` or `$name(...)`, `//` and `#` start no
    comment, a quote no string and a brace no block.

## [0.21.1] — 2026-10-15

### Changed

- **No behavior change: the weekly dependency updates.**
  - Effect: `serde_json` moves from 1.0.151 to 1.0.154 and `@types/node`
    from 26.6.4 to 26.6.5; no source file changed.

## [0.21.0] — 2026-10-11

### Added

- **`kamailio-lsp check --modules-path <dir>` passes the module directory
  to the checker.**
  - Effect: it is passed as `-L`, as the `modulesPath` setting does in the
    editor.
  - Docs: [CLI check mode](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#cli-check-mode).
- **The outline nests `if`, `else`, `switch` and `while` blocks under
  their route.**
  - Effect: blocks are named by their condition; every `loadmodule`,
    `modparam`, top-level parameter and `#!define` is listed.
- **Folding covers blocks inside a route, `socket = { ... }` and `#!ifdef`
  branches.**
  - Effect: multi-line block comments and runs of `#` or `//` comment
    lines fold too.
  - Docs: [Folding](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#folding).
- **Expand selection grows from a word to each enclosing block, the route
  and the file.**
  - Effect: the server answers `textDocument/selectionRange`.
  - Docs: [Selection range](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#selection-range).

### Changed

- **A relative include resolves the way Kamailio resolves it.**
  - Effect: it is looked up in the main config's directory first, where
    `kamailio -c` runs, then beside the including file.
  - Effect: document links follow the same rule.
  - Was: a fragment `inc/a.cfg` that includes `inc/b.cfg` reported its
    routes as undefined.
  - Docs: [Where a relative include opens](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#where-a-relative-include-opens).
- **The formatter indents what a `case` or `default` label guards one
  level under it.**
  - Was: the line after a label was left where it was.
  - Docs: [Formatting](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#formatting).

### Fixed

- **A missing include is reported on the `include_file` line that names
  it.**
  - Was: it was reported at line 1, column 1.
- **`if (!!$var(x))` is code.**
  - Was: any `!!` or `#!` was taken as a preprocessor directive that
    swallowed the rest of the line.
  - Effect: a directive needs a directive keyword, as in Kamailio's lexer;
    the tree-sitter grammar (0.3.1) follows the same rule.
- **`check` reads configs with a latin-1 byte.**
  - Was: it stopped with "cannot read".
- **Workspace diagnostics agree with the include tracking.**
  - Was: a fragment included as `sub/../frag.inc` was reported as a
    program of its own, and a config with a latin-1 byte was skipped.
- **`.inc` and `.m4` fragments edited on disk are noticed.**
  - Was: the file watcher covered only `*.cfg`.
- **Completion offers nothing inside a comment, and single quotes complete
  like double quotes.**
  - Effect: after `loadmodule '`, `modparam('` and `route('` it offers
    what it offers after `"`.
  - Effect: signature help counts arguments across `'a,b'`.
- **Call hierarchy works on a line with non-ASCII text before the route
  name.**
  - Was: the cursor column was read as a byte offset.

## [0.20.0] — 2026-10-11

### Added

- **Failure, onreply and branch routes can be renamed.**
  - Effect: the `t_on_failure("x")`, `t_on_reply("x")` and
    `t_on_branch("x")` strings that arm them follow.
  - Effect: with `#!define R MYROUTE`, renaming `route[MYROUTE]` rewrites
    the define's value; references list it too.
  - Limit: event routes still cannot be renamed.
- **The tree-sitter grammar parses every config in the Kamailio 6.1.4 tree
  that `kamailio -c` accepts.**
  - Effect: nested block comments, directives anywhere, multi-line and
    adjacent strings, the full operator set and selects parse.
  - Effect: IPv4 and IPv6 addresses, `socket = { ... }` blocks,
    `loadmodulex`, `modparamx`, `cfgengine` and `include_file` inside a
    route parse.
  - Do: editors that build the grammar compile `src/scanner.c` along with
    `src/parser.c`.
- **The grammar ships `queries/highlights.scm` and `queries/folds.scm`.**
  - Effect: the Zed setup script downloads the highlight query into the
    extension it builds.
  - Docs: [Zed setup](https://github.com/NormB/kamailio-lsp/blob/main/docs/ZED.md).

### Changed

- **Rename refuses a name that is already defined in the same namespace.**
  - Effect: the refusal names the file that defines it; `kamailio -c`
    rejects the result as a duplicate route.

### Fixed

- **Requests no longer slow down with the square of the file size.**
  - Was: one scanner took 85 s on a 6000-route config; it now takes under
    half a second.
  - Effect: on a 3000-route document inlay hints no longer time out.
- **An open quote or `/*` inside a switched-off `#!ifdef` no longer hides
  the file's rest.**
  - Effect: the string or comment ends at the next conditional directive,
    as in the preprocessor.
  - Effect: Kamailio's `misc/examples/pkg/sipcapture.cfg` lost four false
    "route 'STORE' is not defined" warnings.
- **One route in both the `#!ifdef` and `#!else` branch of a conditional
  is no duplicate.**
  - Was: the route was reported as defined twice.
  - Limit: under two separate conditionals it still is.
- **Defines are applied the way the preprocessor applies them.**
  - Effect: `#!trydef` keeps a value already bound, `#!redefine` replaces
    it, and a quoted value (`#!define R "MYROUTE"`) names the bare route.
  - Effect: `#!ifexp` opens a level, so its `#!endif` no longer closes the
    `#!ifdef` around it.
  - Effect: `#!def`, `#!trydefine` and `#!redef` count as defining for
    include tracking.
- **Formatting a CRLF file no longer doubles its carriage returns.**
  - Was: every reformatted line ended in `\r\r\n`.
- **`kamailio-lsp check --bin ... fn.cfg` works with a relative path.**
  - Was: a bare file name exited 2 with "cannot run", and `sub/fn.cfg` was
    handed to the checker from inside `sub/`.
- **Nested block comments are comments.**
  - Effect: Kamailio nests `/* */`;
    `/* a /* b */ route[GONE] { exit; } */` no longer defines `GONE`.

## [0.19.7] — 2026-10-08

### Changed

- **No behavior change: the weekly dependency updates.**
  - Effect: `tokio` moves from 1.53.1 to 1.53.2 and `@types/node` from
    26.6.3 to 26.6.4; no source file changed.

## [0.19.6] — 2026-10-01

### Changed

- **No behavior change: one weekly dependency update.**
  - Effect: `vscode-languageclient` moves from 10.1.1 to 10.1.2 and
    `@types/node` from 26.6.2 to 26.6.3; no source file changed.

### Security

- **`brace-expansion` moves from 5.0.9 to 5.0.12, which fixes three
  advisories.**
  - Was: the extension bundles it through `minimatch`, and 5.0.9 is
    affected by three advisories published 2026-09-29.
  - Effect: GHSA-6j4f-fj2g-mc7p (high, fixed in 5.0.10) and
    GHSA-qhr7-859c-m2p7 (high, fixed in 5.0.11) no longer apply.
  - Effect: GHSA-q2hr-2g5m-vwhr (moderate, fixed in 5.0.12) no longer
    applies.

## [0.19.5] — 2026-09-26

### Changed

- **No behavior change: one dependency update.**
  - Effect: `@types/node`, a compile-time stub, moves from 26.6.1 to
    26.6.2; no source file or Rust dependency changed.

## [0.19.4] — 2026-09-15

### Changed

- **No behavior change: the weekly dependency updates.**
  - Effect: `@types/node`, a compile-time stub, moves from 26.4.1 to
    26.6.1.
  - Effect: five transitive Rust crates are updated: `bitflags` to 2.13.2,
    `crossbeam-utils` to 0.8.23, `mio` to 1.2.3, `smallvec` to 1.16.1 and
    `syn` to 3.0.5.

## [0.19.3] — 2026-09-09

### Fixed

- **`kamailioLsp.versionInHints` and `kamailioLsp.kamailioVersion` now
  take effect.**
  - Was: the extension never passed either setting to the server, so
    setting one changed nothing.
  - Do: set them in Settings instead of the
    `KAMAILIO_LSP_VERSION_IN_HINTS` and `KAMAILIO_LSP_VERSION` environment
    variables.
  - Docs: [Admin guide: kamailioVersion](https://github.com/NormB/kamailio-lsp/blob/main/docs/ADMIN.md#kamailioversion-string).

## [0.19.2] — 2026-09-09

### Changed

- **No behavior change: one compile-time dependency update.**
  - Effect: `@types/node` moves from 26.4.0 to 26.4.1; nothing that runs
    in the editor changed.

## [0.19.1] — 2026-09-02

### Changed

- **`vscode-languageclient` moves from 10.1.0 to 10.1.1, which carries LSP
  3.18.3.**
  - Effect: diagnostic pull state survives a document being closed and
    reopened quickly, and the client's stream handlers are configurable.
  - Effect: `@types/node`, a compile-time stub, moves from 26.2.0 to
    26.4.0.

## [0.19.0] — 2026-08-26

### Added

- **A core hover shows the cookbook's example of usage.**
  - Effect: 209 parameters, 18 functions and 36 pseudo-variables carry
    their example.
  - Limit: no separate `Default:` line is added; the cookbook writes the
    default into the first paragraph.
- **`listen=` modifiers and `socket = { }` attributes hover and
  complete.**
  - Effect: `advertise`, `name` and `virtual` apply on a `listen` line.
  - Effect: `bind`, `advertise`, `name`, `agname`, `workers`, `virtual`
    and `vrf` apply inside the brace form.
  - Limit: `workers` is offered although the cookbook's attribute list
    omits it, and its hover says so.
- **Thirty-five spellings the cookbook skips hover as the thing they
  are.**
  - Effect: examples are `wdir` for `workdir`, `seturi` for `rewriteuri`
    and `dns_use_cache` for `use_dns_cache`.
  - Effect: an alias of a call is offered as a call, not a parameter.
- **Twenty-nine globals no page describes are offered, and say so.**
  - Effect: the grammar accepts them and the cookbook never mentions them.

### Fixed

- **A heading naming several parameters yields all of them.**
  - Was: `### tcp_source_ipv4, tcp_source_ipv6` produced a parameter named
    `tcp_source_ipv4,`, and its siblings were missing.
- **A `socket = { ... }` written on one line closes on that line.**
  - Was: the block stayed open for the rest of the file, and `name`,
    `virtual`, `workers` and `advertise` hovered as socket syntax in route
    bodies.
  - Effect: an attribute on the opener's own line hovers too.
- **A `#` at column zero inside an example is a configuration comment, not
  a heading.**
  - Was: a section ending at one lost the rest of its example.

## [0.18.0] — 2026-08-26

### Added

- **Routing blocks and control statements hover.**
  - Effect: `request_route`, `branch_route` and the other six hover, as do
    `if`, `switch`, `while`, `else`, `case` and `default`.
  - Was: control keywords completed as the word "keyword" with nothing
    under it.
- **Typing `xlog(` offers the log levels, quoted.**
  - Effect: `L_ALERT`, `L_BUG`, `L_CRIT`, `L_ERR`, `L_WARN`, `L_NOTICE`,
    `L_INFO` and `L_DBG` are read from the tree's
    `src/modules/xlog/xlog.c`.
  - Effect: `xlogl` and `xlogm` offer them too; a `$` still offers
    pseudo-variables.
  - Limit: `xdbg`, `xinfo`, `xerr`, `xnotice` and `xwarn` carry their
    level in the name and offer none.
- **Ctrl+Alt+H turns hovers and completion off and on (Cmd+Alt+H on
  macOS).**
  - Effect: it applies without a restart; the status bar reads
    `Kamailio hints off`, and diagnostics are left alone.
  - Do: the setting behind it is `kamailioLsp.assistance`.
  - Docs: [Turning the popups off](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#turning-the-popups-off).

### Fixed

- **Thirty-five pseudo-variables that hovered blank now show their text.**
  - Was: `$avp`, `$var`, `$hdr`, `$sht` and thirty-one more had an entry
    and no text.

## [0.17.0] — 2026-08-25

### Changed

- **The release is no longer under every hover and completion item.**
  - Was: each hover repeated the release that the status bar and the
    release warnings already name.
  - Do: set `versionInHints` to turn the note back on.
  - Docs: [Admin guide: versionInHints](https://github.com/NormB/kamailio-lsp/blob/main/docs/ADMIN.md#versioninhints-boolean).
- **Choosing a release is documented.**
  - Effect: `GETTING_STARTED.md` covers the dropdown, `settings.json`, the
    environment variable `check` reads, and pointing at your own tree.
  - Docs: [Checking against a specific Kamailio release](https://github.com/NormB/kamailio-lsp/blob/main/docs/GETTING_STARTED.md#checking-against-a-specific-kamailio-release).

### Fixed

- **The release note names the catalog it describes and the release in
  use.**
  - Was: it named the release the vendored file carried, whatever
    `kamailioVersion` selected.
  - Limit: core documentation is one vendored artifact at 6.1.x and does
    not follow `kamailioVersion`; the note says so.

## [0.16.0] — 2026-08-25

### Added

- **Choose the release you run: the catalog covers Kamailio 5.8.8, 6.0.7
  and 6.1.4.**
  - Do: pick the release with `kamailioLsp.kamailioVersion`, a dropdown of
    the releases the shipped catalog covers.
  - Effect: a warning names that release, and the releases where the
    parameter exists, as in "it exists in 5.8.8, 6.0.7".
  - Docs: [Admin guide: kamailioVersion](https://github.com/NormB/kamailio-lsp/blob/main/docs/ADMIN.md#kamailioversion-string).
- **The status bar names the release in use.**
  - Effect: it shows the catalog `modparam` lines are checked against,
    such as `Kamailio 6.1.4`, or the configured source tree.

### Changed

- **`modparam` parameters come from each module's own C table.**
  - Effect: 243 parameters no README mentions are available, including
    `msilo`'s `sc_status` and `ndb_cassandra`'s `port`.
  - Effect: 19 entries the READMEs invented are gone, among them `slack`'s
    `slack url`.
  - Effect: `kazoo`'s `amqp_consumer_ack_timeout_sec` and `_micro` are
    accepted; `amqp_consumer_ack_timeout`, never exported, is no longer
    offered.
- **`kamailio-lsp check` runs the same `modparam` check the editor does.**
  - Was: a CI job or git hook ran only the analyzer, so an unknown
    parameter passed there.
  - Effect: `check` reports warnings it did not report before.

### Fixed

- **A warning no longer has a gap in the middle of the sentence.**
  - Was: a line continuation kept its indentation inside the message.

## [0.15.1] — 2026-08-25

### Fixed

- **A fragment gets the language whatever it is called.**
  - Was: the extension asked the server only about files named `*.cfg`,
    so `include/*.inc` fragments got no colors, completion or diagnostics.
  - Effect: any plain-text file is asked about; one nothing includes is
    left alone.
- **The workspace sweep looks for `.inc` and `.m4` files too.**
  - Was: it collected `*.cfg` only, so fragments in other trees resolved
    no root and were analyzed alone.
  - Limit: it is a search for configurations, not a read of every file in
    the folder.
- **Sixty-one parameters the READMEs document are back in the catalog.**
  - Was: the server reported each as a parameter that does not exist.
  - Effect: `kazoo`'s thirty grouped parameters and the database
    parameters of `carrierroute` and `matrix` are read.
  - Effect: `ims_qos` and `ims_qos_npn` parameters and nine `rtpengine`
    parameters, `ping_interval` and `enable_dmq` among them, are read.
- **A module whose documentation could not be read no longer causes
  `modparam` warnings.**
  - Effect: an empty parameter list is not taken as evidence; a module
    that exports functions but no parameters is still checked.

## [0.15.0] — 2026-08-25

### Added

- **An included file opened on its own gets the language.**
  - Effect: the extension asks the server whether anything includes the
    file; files another extension claims are left alone.
  - Do: turn it off with `kamailioLsp.associateIncludedFiles`.
  - Limit: the extension still does not claim every `.cfg` file.
- **An included file is analyzed as part of its root.**
  - Effect: routes, modules and defines of the parent are in scope, so
    they are not reported as undefined.
  - Effect: `kamailio -c` runs on the root, and each error goes to the
    file it names.
  - Do: open the folder, not the single file; the root is found by reading
    the configs under the workspace folders.
  - Limit: a workspace past 500 configs, or a config too large or
    unreadable, is reported in the output channel.
  - Docs: [An included file opened on its own](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#an-included-file-opened-on-its-own).
- **Navigation spans the whole configuration from inside a fragment.**
  - Effect: go to definition, references, rename, call hierarchy,
    workspace symbols, reference counts and route completion cross the
    include boundary.
- **`kamailio-lsp check` finds the root the same way the editor does.**
  - Effect: a correct split configuration no longer fails under
    `--strict`.
- **New request `kamailio/analysisRoot` for editors other than VS Code.**
  - Effect: it returns the root's URI for a document, or `null` when the
    document is a program in its own right.
- **A file behind an unmet `#!ifdef` is not part of the configuration.**
  - Effect: Kamailio never opens it, so the config that would have
    included it does not claim it.
  - Limit: OpenSIPS differs, and its extension differs to match.

### Fixed

- **A failed `kamailio -c` run that reported no position no longer names
  an empty file.**
  - Was: the message began `check failed in , line 1:`, a file and a line
    the parser never gave.

## [0.14.4] — 2026-08-24

### Changed

- **Documentation only: the pages state what the extension does with your
  configuration.**
  - Effect: the listing, the README and `SECURITY.md` state that the
    server links no HTTP client and runs only your `kamailio` binary.
  - Effect: `SECURITY.md` puts any path by which configuration content,
    file paths or environment values reach the network in scope.
  - Limit: `trace.server` echoes cfg text into the editor's output
    channel, and the editor's own telemetry is outside the server's
    control.
  - Docs: [Data handling](https://github.com/NormB/kamailio-lsp/blob/main/SECURITY.md#data-handling).

## [0.14.3] — 2026-08-24

### Fixed

- **The tree-sitter grammar can be built by the editors it is offered
  to.**
  - Was: the generated `src/parser.c` was in no commit, and
    nvim-treesitter, Helix and Zed build from that file.
  - Limit: nothing changes for VS Code users.
- **The Zed guide points at the public repository directly.**
  - Was: it cloned the repository to give the grammar directory a git
    root of its own, which Zed does not need.
  - Effect: Zed takes a `path` for a grammar in a subdirectory, as do
    Helix (`subpath`) and nvim-treesitter (`location`).
  - Docs: [Zed setup](https://github.com/NormB/kamailio-lsp/blob/main/docs/ZED.md).

## [0.14.2] — 2026-08-24

### Changed

- **No behavior change: the extension's dependency tree is pinned by a
  lockfile.**
  - Effect: the server behaves exactly as 0.14.1 did; release builds
    install the client's dependencies from the lock.

## [0.14.1] — 2026-08-24

### Added

- **Every editor is documented, not just VS Code.**
  - Effect: `docs/EDITORS.md` has worked examples for Neovim, coc.nvim,
    Helix, Emacs, Vim, Sublime Text, Kate and JetBrains (via LSP4IJ).
  - Effect: it also covers `check` in CI or a git hook with no editor.
  - Docs: [Editor and tool setup](https://github.com/NormB/kamailio-lsp/blob/main/docs/EDITORS.md).
- **Zed has a step-by-step guide.**
  - Effect: `docs/ZED.md` builds the small WebAssembly extension Zed needs
    to run a language server.
  - Effect: its language config carries `first_line_pattern`, so a config
    opening with `#!KAMAILIO`, `#!OPENSER`, `#!SER`, `#!MAXCOMPAT` or
    `#!ALL` is recognized.
  - Docs: [Zed setup](https://github.com/NormB/kamailio-lsp/blob/main/docs/ZED.md).

### Fixed

- **The editor examples match the files the extension claims.**
  - Was: they named only `kamailio.cfg` and `#!KAMAILIO`, told Helix to
    claim every `.cfg` file, and presented `kamailioWiki` as the way to
    get documentation.
  - Limit: documentation only; the server behaves exactly as 0.14.0 did.

## [0.14.0] — 2026-08-24

### Added

- **Every module's functions and parameters ship with the extension.**
  - Effect: 254 modules, 1224 functions and 2185 parameters harvested from
    the Kamailio 6.1.4 tree complete with nothing configured.
  - Effect: the core language comes from the 6.1.x cookbook and the
    modules from the 6.1.4 tree; hover names the version.
  - Limit: module functions appear only in a config that `loadmodule`s
    them; the list is what 6.1.4 documents, not what your system has.
  - Do: set `kamailioSrc` (or `kamailioWiki`) and your own checkout
    replaces the matching built-in catalog; the two are never merged.
  - Docs: [Documentation before you configure anything](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#documentation-before-you-configure-anything).

### Fixed

- **Hover and signature help read the syntax before choosing an entry.**
  - Was: a global `log_facility=LOG_LOCAL0` hovered as a module's
    `log_facility` parameter, and a module not loaded could shadow the
    core language.
  - Effect: the global is the core parameter, and `log_facility` inside
    `modparam("acc", ...)` is that module's.

## [0.13.1] — 2026-08-24

### Fixed

- **The getting-started guide names every file the extension claims.**
  - Was: its "No colors" row named only `kamailio.cfg` and `#!KAMAILIO`.
  - Effect: `kamailio*.cfg`, `*.kamailio.cfg` and the markers `#!OPENSER`,
    `#!SER`, `#!MAXCOMPAT` and `#!ALL` are listed.
- **`README.md` and the guide say the core language is documented out of
  the box.**
  - Was: they presented `kamailioWiki` as the only way to get
    core-language documentation, wrong since 0.13.0.
  - Limit: documentation only; the server behaves exactly as 0.13.0 did.

## [0.13.0] — 2026-08-24

### Added

- **The core language completes out of the box.**
  - Effect: a catalog harvested from the 6.1.x cookbook ships: 43
    functions, 222 parameters and 292 pseudo-variables.
  - Was: `debug`, `children`, `listen` and the rest offered nothing until
    kamailio-wiki was cloned.
  - Do: set `kamailioWiki` and your own checkout wins; hover on a built-in
    entry says which version it came from.
  - Limit: module documentation still needs a source tree.

### Fixed

- **`exit` no longer completes as `exit()`.**
  - Was: with a checkout configured it inserted a call snippet for a
    statement written `exit;`.
- **Hover no longer comes back empty for `exit` and similar keywords.**
  - Was: a bare keyword beat the documented entry by arriving first.

## [0.12.0] — 2026-08-24

### Added

- **`kamailio*.cfg` and `*.kamailio.cfg` are recognized.**
  - Effect: `kamailio-local.cfg` and split configs get language support.
  - Effect: the first-line rule covers `#!SER`, `#!KAMAILIO`, `#!OPENSER`,
    `#!MAXCOMPAT` and `#!ALL`.
  - Limit: the generic `.cfg` extension is still not claimed.

### Changed

- **The installers say that a sideloaded extension receives no updates.**
  - Effect: the installers and the Getting Started guide state it and name
    the two ways out.

### Fixed

- **The 0.11.3 note is corrected.**
  - Was: it said only files named exactly `kamailio.cfg` were claimed; a
    config opening with `#!KAMAILIO` already worked under any name.

## [0.11.3] — 2026-08-23

### Changed

- **The listing says which files the extension recognizes.**
  - Effect: it says only files named exactly `kamailio.cfg` are claimed,
    and how to widen that with `files.associations`.
  - Limit: 0.12.0 corrects this note; a config opening with `#!KAMAILIO`
    already worked under any name.

## [0.11.2] — 2026-08-23

### Fixed

- **The marketplace listing describes what the extension does.**
  - Was: formatting, call hierarchy, preprocessor symbols, inlay hints,
    include links, pull diagnostics, watched files and refactorings were
    not mentioned.
  - Effect: `docs/ADMIN.md` gains the missing `inlayHintParameterNames`
    and `inlayHintDefineValues` options.

## [0.11.1] — 2026-08-23

### Fixed

- **The formatter keeps the indentation of continuation lines.**
  - Was: on a 993-line production config it wanted to change 25 lines,
    all wrongly, including the body of a braceless `if`.
  - Effect: a line is re-indented only when the previous code line ended
    a statement.

## [0.11.0] — 2026-08-23

### Added

- **Extract into a route: selected lines move into a `route[EXTRACTED]` of
  their own.**
  - Effect: a `route(EXTRACTED);` call is left behind at the same
    indentation.
  - Limit: it refuses unbalanced braces, a selection covering the block's
    own braces, and one containing `return`.
  - Docs: [Refactorings](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#refactorings).
- **Remove duplicate `loadmodule` lines.**
  - Effect: every occurrence after the first is removed and nothing is
    reordered; a second load of a module is a parse error.

## [0.10.0] — 2026-08-23

### Added

- **Pull diagnostics: `textDocument/diagnostic` and
  `workspace/diagnostic`.**
  - Effect: the workspace sweep reports problems across the project
    without opening a file; an unchanged document comes back `unchanged`.
  - Effect: only root configs appear in the sweep; a config another
    config includes is covered by its root.
  - Effect: pushing stops when the client pulls, so nothing is reported
    twice.
  - Limit: the sweep stops at 500 configs and logs that it did.
  - Docs: [Pull diagnostics](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#pull-diagnostics).

## [0.9.0] — 2026-08-23

### Added

- **Watched files: an include, module tree or wiki checkout changing on
  disk is noticed.**
  - Effect: a git checkout, a rebuild or another tool re-checks and
    re-harvests without the buffer being touched.
  - Effect: a re-check driven by a watched file publishes even when the
    result is clean, so a stale warning is cleared.
  - Docs: [Watched files](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#watched-files).

## [0.8.0] — 2026-08-23

### Added

- **Inlay hints label arguments with documented parameter names.**
  - Effect: `t_relay("udp", 1)` reads as
    `t_relay(flags: "udp", outbound_proxy: 1)` without the document
    changing.
  - Limit: only calls the catalog knows are hinted, and only as far as
    the signature goes.
  - Do: `kamailioLsp.inlayHints.parameterNames` turns them off, live.
  - Docs: [Inlay hints](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#inlay-hints).
- **Inlay hints show what a `#!define`d name expands to.**
  - Effect: every use carries its value, including inside `#!ifdef`.
  - Limit: the definition site is not hinted, and a name inside a string
    is not a use.
  - Do: `kamailioLsp.inlayHints.defineValues` turns them off, live.

## [0.7.0] — 2026-08-23

### Added

- **Preprocessor symbols: the server reads `#!define` and its relatives.**
  - Effect: hover a name for what it binds, Ctrl+Click to jump there,
    complete it where code is legal, and find it in the outline.
  - Effect: every defining form in Kamailio's `cfg.lex` is recognized
    behind `#!` or `!!`, and backslash-continued directives keep their
    whole value.
  - Docs: [Preprocessor symbols](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#preprocessor-symbols).

### Fixed

- **A route reached through a `#!define` alias is no longer flagged as
  undefined.**
  - Was: `#!define RELAY MYROUTE` with `route(RELAY)` was flagged;
    Kamailio accepts that config.
  - Effect: the analyzer expands through the define table, across included
    files, before deciding.
  - Limit: if the expansion names no route, the warning names both the
    alias and what it expands to.

## [0.6.0] — 2026-08-23

### Added

- **Call hierarchy: Shift+Alt+H on a route name opens the route call
  graph.**
  - Effect: it shows who calls `route[X]` and what `route[X]` calls,
    across the include closure.
  - Effect: a route called but defined nowhere is listed as an outgoing
    edge, marked `undefined`.
  - Limit: asking for the callers of `request_route` or `failure_route`
    declines; they are armed by the core or a module-function string.
  - Docs: [Call hierarchy](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#call-hierarchy).

## [0.5.0] — 2026-08-23

### Added

- **Formatting: Shift+Alt+F and format-on-save re-indent a `kamailio.cfg`
  by brace depth.**
  - Effect: `textDocument/formatting` and `textDocument/rangeFormatting`
    strip trailing whitespace and follow the editor's tab settings.
  - Effect: `#!` and `!!` directives keep their column, and a
    backslash-continued `#!define` keeps its own layout.
  - Limit: it rewrites only the leading and trailing whitespace of a
    line; it never joins, splits or reorders lines.
  - Docs: [Formatting](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#formatting).

## [0.4.1] — 2026-08-23

### Fixed

- **The README names features that shipped earlier.**
  - Was: it did not mention include links, `prepareRename`,
    `semanticTokens/range` or live reconfiguration.
- **`KAMAILIO_LSP_TRACE_INDEX` is documented.**
  - Was: the server read this environment variable all along and no page
    mentioned it.

## [0.4.0] — 2026-08-23

### Changed

- **The language-server stack moves to `tower-lsp-server`.**
  - Was: `tower-lsp` has had no release since 0.20.0 (August 2023).
  - Effect: no behavior changes; the server builds from 66 dependencies
    instead of 95.
- **Proven against Kamailio 6.1.4, the current stable line, alongside
  6.0.x.**
  - Effect: the 6.1.4 diagnostic shapes are captured verbatim beside the
    6.0.1 ones.
- **Nothing pins a Kamailio version.**
  - Effect: module docs come from the tree you point at and core docs from
    the newest stable cookbook in your wiki checkout; `devel` never wins.

## [0.3.0] — 2026-08-20

### Added

- **Semantic tokens answer range requests.**
  - Effect: `textDocument/semanticTokens/range` is served alongside
    `full`, so editors can request the visible slice of a large config.
- **Prepare rename: F2 is validated before the rename box opens.**
  - Effect: renamable route names get an exact range and placeholder;
    off-symbol and per-kind names such as `event_route` are blocked.
- **Include links: `include_file`/`import_file` paths are clickable
  document links.**
  - Effect: paths resolve against the including file's directory; absolute
    paths pass through and missing targets still link.
  - Docs: [Include links](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#include-links).
- **Five settings apply without restarting the language server.**
  - Effect: `diagnostics.analyzer`, `completion.snippets`,
    `codeLens.references`, `diagnostics.maxProblems` and `checkTimeoutMs`
    apply live, and open files are re-diagnosed.
  - Limit: path settings (server and kamailio binary, source and wiki
    trees, cache dir) still restart it automatically.
- **The documentation harvest reports progress.**
  - Effect: supporting editors show a busy indicator
    (`workDoneProgress`); a configured tree that yields zero symbols
    raises a warning naming the path.

### Changed

- **The server runs `kamailio -c` from the configuration's own
  directory.**
  - Effect: relative `include_file`/`import_file` paths resolve
    identically in editor and CI runs.
- **A newer save supersedes an in-flight `kamailio -c` run on the same
  document.**
  - Effect: the stale child process is ended instead of blocking the fresh
    check.

### Fixed

- **Editing a documentation file re-harvests the catalog.**
  - Was: only directory-level changes did.

## [0.2.1] — 2026-08-20

### Changed

- **No functional changes: internal release checks only.**
  - Effect: nothing in the server or the extension behaves differently.

## [0.2.0] — 2026-08-20

### Added

- **Workspace symbols: Ctrl+T searches route definitions.**
  - Effect: the search spans every open file and its include closure,
    case-insensitive and capped.
- **Code lenses show a reference count on named callable `route` blocks.**
  - Effect: counts span the include closure.
  - Limit: `failure_route` and similar blocks carry none; their arming is
    not countable through `route()`.
  - Do: the setting is `kamailioLsp.codeLens.references`.
- **Quick fixes: "Load module 'X'" and "Create route[x]".**
  - Effect: the first appears when the parser says
    `unknown command, missing loadmodule?`; the second stubs an undefined
    target with an `exit;` body.
- **Catalog-pinned validation of `modparam` parameters.**
  - Effect: parameters the configured source tree does not document warn
    as you type; unknown modules stay silent.
  - Effect: `db_url(str)` headings, with no space before the type, now
    parse.
- **Semantic tokens for route names and pseudo-variables.**
  - Effect: pseudo-variables are marked inside single- or double-quoted
    strings, which interpolate identically.
- **CLI: `kamailio-lsp check [--strict] [--bin <kamailio>] <file>...`.**
  - Effect: analyzer and real-parser findings print as `file:line:col`
    lines; exit codes 0/1/2 serve CI and git hooks.
  - Docs: [CLI check mode](https://github.com/NormB/kamailio-lsp/blob/main/docs/FEATURES.md#cli-check-mode).

## [0.1.3] — 2026-08-20

### Changed

- **References and rename cross the include closure.**
  - Effect: renaming a route defined in an `include_file` rewrites every
    file, and Shift+F12 lists the include's definition.
- **`route(X)` is satisfied only by `route[X]` blocks.**
  - Effect: a `failure_route[X]` no longer masks an undefined target;
    duplicate-definition warnings are per kind.
  - Effect: numeric `route(N)` calls never warn, and `route(` completion
    offers only callable names.
  - Limit: renaming `failure_route`, `branch_route` and similar names is
    rejected with an explanation.

### Fixed

- **Single-quoted strings are recognized everywhere.**
  - Effect: `loadmodule 'tm.so'`, `route['SQ']` and `xlog('hi')` work in
    analysis, completion, tree-sitter and TextMate highlighting.
  - Effect: Kamailio's semantics apply: no escapes, and a string may span
    lines.
- **Signature help labels split on top-level commas only.**
  - Effect: nested calls, bracket groups (`t_relay([host, port])`) and
    quoted commas stay whole.

## [0.1.2] — 2026-08-20

### Changed

- **Rename accepts only names Kamailio parses unquoted.**
  - Effect: new names must match `[A-Za-z_][A-Za-z0-9_]*`.
  - Limit: event-route names (`event_route[mod:event]`) cannot be renamed;
    references and highlights on them keep working.
- **All load forms are recognized.**
  - Effect: `loadmodule("x.so")`, `loadmodule("x", "opts")`, `loadmodulex`
    and `modparamx` are read.
  - Effect: `xlog` is no longer offered as a core keyword; it is a module
    function.
- **The grammars match the lexer.**
  - Effect: tree-sitter gains `and`/`or`/`not`/`mod`/`|`/`&`, paren-less
    `return`, `!!` directives and `//` comments.
  - Effect: tree-sitter drops `!~`/`%`/`+=`/`-=`/`break(arg)`/named
    `request_route`; the TextMate directive set is the real one.

### Fixed

- **Errors inside included files appear in the editor.**
  - Was: Kamailio reports them against the include path as written, which
    was filtered out, so a broken include showed a clean editor.
  - Effect: they attach to the `include_file` directive, with the real
    file and line in the message.
- **`#!` and `!!` directives are no longer treated as comments.**
  - Effect: multi-line `#!define` bodies leak no phantom routes or
    modules, and `#!include_file` and `!!import_file` are followed.
  - Effect: `//` line comments are recognized.
- **Six more modules are documented.**
  - Effect: nat_traversal, call_control, kafka, keepalive, lrkproxy and
    seas now harvest.
- **The documentation's diagnostics demo is corrected.**
  - Effect: it misspells a real parameter (`fr_tmer` for tm's `fr_timer`)
    and quotes the message shown, `Can't set module parameter`.

### Security

- **Untrusted workspaces also restrict `kamailioLsp.serverPath` and
  `kamailioLsp.modulesPath`.**
  - Effect: this closes a workspace-settings code-execution vector.

## [0.1.1] — 2026-08-20

### Fixed

- **The documentation uses Kamailio's lowercase `-c` check flag
  everywhere.**
  - Was: a leftover uppercase `-C` was OpenSIPS's flag.
  - Effect: the example diagnostic quotes Kamailio's message,
    `parameter <fr_timeot> of type <2:int> not found in module <tm>`.

## [0.1.0] — 2026-08-20

### Added

- **Initial release: diagnostics via `kamailio -c --all-errors`.**
  - Effect: diagnostics carry line and column ranges, are versioned
    against the buffer, time-bounded and output-capped.
- **Context-sensitive completion, hover, go to definition and document
  symbols.**
  - Effect: modules, parameters, functions of loaded modules, core
    functions and parameters, and pseudo-variables after `$` complete.
  - Effect: documentation is harvested from a Kamailio source tree and a
    kamailio-wiki checkout, cached per pair.
- **Diagnostics stay off in untrusted folders.**
  - Effect: checking a config loads its modules, so an untrusted folder
    gets no diagnostics.
  - Effect: settings changes restart the server live.
- **Static snippets and tabstop snippets.**
  - Effect: static snippets cover route blocks, preprocessor directives,
    `loadmodule`/`modparam`, control flow, `xlog` and `sl_send_reply`.
  - Effect: function completions insert tabstop snippets.
- **Signature help.**
  - Effect: the innermost unclosed call's signature shows on `(` and `,`,
    with the active parameter highlighted.
  - Effect: module exports are looked up first, then core functions;
    commas inside strings do not advance the parameter.
- **References, rename and highlights for route names.**
  - Effect: Shift+F12 lists every call site and the definition; F2
    renames everywhere and rejects illegal names.
  - Effect: occurrences highlight, with the definition as a write.
- **Include awareness: `include_file`/`import_file` are followed.**
  - Effect: completion sees included modules and routes, and go to
    definition jumps into the include.
  - Effect: open buffers are preferred over disk, cycles are safe, and
    dotted and colon-carrying route names resolve whole.
- **Analyzer warnings appear between saves, debounced as you type.**
  - Effect: undefined `route()` targets and duplicate route definitions
    are reported.
  - Do: the setting is `kamailioLsp.diagnostics.analyzer`.
- **Folding for every route-family block.**
  - Effect: the outline and breadcrumbs carry full block extents.
- **Completion collapses duplicate labels and replaces a typed `$token`.**
  - Effect: duplicate labels collapse to the most informative item, and
    `route(` completes route names.
- **Tree-sitter grammar (`tree-sitter-kamailio`) with corpus tests.**
  - Effect: the grammar ships in the repository as `tree-sitter-kamailio`.
- **Proven against Kamailio 6.0.x.**
  - Effect: binary 6.0.1, the branch 6.0 tree and the 6.0.x wiki cookbooks
    were used.
