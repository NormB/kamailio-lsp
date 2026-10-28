# Contributing to kamailio-lsp

## The one hard rule: TDD

Every behavior change lands test-first: write the failing test, watch
it fail, implement, watch it pass. PRs that add behavior without new
tests will be asked to add them. Parsing-adjacent code (cfg text,
module docs, `kamailio -c` output) must also cover adversarial input:
empty, NUL bytes, backslashes, truncated constructs.

## Ground truth: the binary and the grammar, never memory

Kamailio's config language is defined by `src/core/cfg.y` /
`src/core/cfg.lex` and by what an actual `kamailio` binary accepts —
not by OpenSIPS habits, docs, or recollection. The rules:

- **Cite or capture.** Any assertion about the language — in code,
  tests, comments, or docs — must cite a grammar source line
  (`cfg.y` / `cfg.lex`) or a live capture from the binary. If you
  cannot point at one, run the experiment first.
- **Fixtures are captured, never composed.** Test fixtures for parser
  output (`kamailio -c` stderr) are pasted verbatim from a real run
  and dated. Hand-written "plausible" output is how drift starts.
- **The differential gates are the backstop.** `ground_truth_test.rs`
  proves the analyzer stays silent on every corpus config the real
  binary accepts and that rename round-trips through the real parser;
  `corpus_test.rs` proves rejected configs are never silent. Run them
  before merging anything language-adjacent:
  `KAMAILIO_LSP_TEST_TREE=... KAMAILIO_LSP_TEST_BIN=... cargo test`.
- **Re-audit on every new upstream release.** The ground truth moves
  with Kamailio: when a new major/minor lands, re-run the deep audit
  (docs vs binary, grammar vs code, diff against opensips-lsp for
  unported fixes) with the new tree and binary in the gate envs.

## Local workflow

Turn on the git hooks once per clone:

```sh
git config core.hooksPath .githooks
```

They run `scripts/gate.sh`, the checks CI runs, before a commit and
before a push. The commit hook refuses a message or a staged line that
credits an AI tool, and then runs rustfmt, clippy on both packages
with warnings as errors, and every test. It runs them on the working
tree, so it also refuses a commit while anything is unstaged or
untracked. The push hook refuses a commit without a good signature,
or one whose author or committer is not you. It then runs rustdoc with
warnings as errors, the MSRV build, the client's compile and tests, and
`cargo audit`. `scripts/test-hooks.sh` tests the hooks themselves.

The tests need the proof environment. Write it once where the gate
reads it:

```sh
scripts/proof-env.sh > "$(git rev-parse --git-common-dir)/env.sh"
```

Rust code follows the [Google Rust Style
Guide](https://google.github.io/styleguide/rustguide.html), which
builds on the [Rust Style Guide](https://doc.rust-lang.org/nightly/style-guide/)
and the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/).
`rustfmt.toml` sets its options, and some of them are unstable, so
format with `scripts/fmt.sh fix`. It runs rustfmt from a pinned
nightly. The crate itself builds on stable. Clippy denies the guide's
lints through the `[lints]` tables in both manifests, and
`tests/style_test.rs` checks the rest.

```sh
scripts/gate.sh commit         # rustfmt check, clippy, the full suite
scripts/gate.sh push           # rustdoc, MSRV, client, cargo audit
scripts/fmt.sh fix             # format with the pinned nightly
```

The real-tree harvest test is opt-in:
`KAMAILIO_LSP_TEST_TREE=/path/to/kamailio cargo test`.

## Fuzzing

Each target in `fuzz/fuzz_targets/` feeds random input, grown from its
seeds, to one thing the server reads from outside:

| Target | What it reads |
| --- | --- |
| `cfg` | any text as a cfg document: the scanner, the outline and folds, the formatter. Every position lies inside the document, no block ends before it starts, and formatting a formatted document changes nothing |
| `readme` | any text as a module README, as the harvester reads them; every item it gives has a name |
| `check_output` | any text as `kamailio -c` output, with any exit status. No range is reversed or empty, no message passes the cap, and a failed check always reports something |

Fuzzing needs the nightly toolchain named in `fuzz/rust-toolchain.toml`
and [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) 0.13.2:

```sh
cargo install cargo-fuzz --version 0.13.2 --locked
scripts/fuzz.sh list                  # the targets
scripts/fuzz.sh run cfg 60            # run one target for 60 seconds
scripts/fuzz.sh plant cfg             # plant a panic, expect the target to find it
```

`scripts/fuzz.sh run` reads the seeds in `fuzz/seeds/<target>/` and, for
`cfg`, the cfg files in `tests/grammar/` and the grammar's tests. What
libFuzzer adds goes to `fuzz/corpus/<target>/`, a crash input to
`fuzz/artifacts/<target>/`; git ignores both. A crash is a defect: fix it
with a regression test written first.

`scripts/fuzz.sh plant` checks the targets themselves: for each line of
`fuzz/plants.txt` it puts `panic!("planted fault");` before one line of
the code a target reads, runs the target, and expects the crash at that
line. It then puts the file back and compares it with its copy, byte for
byte. CI runs every target for 60 seconds and then its plant;
ClusterFuzzLite (`.clusterfuzzlite/`) fuzzes pull requests, pushes to
`main` that change `src/` or `fuzz/`, and every target for an hour each
Monday.

## PRs

- Feature branches, one problem per PR, linear history (squash or
  rebase merges).
- `docs/ADMIN.md` documents every user-facing option and its
  structure is enforced by a test — update it when options change.
- The VS Code client (`client/`) compiles in CI with `tsc`; keep its
  settings in sync with the server's initialization options.

## Reporting bugs

Report a bug or ask for a feature with the issue forms at
<https://github.com/NormB/kamailio-lsp/issues/new/choose>. The "Bug report"
form asks what happened, a minimal `kamailio.cfg` or the steps that show it,
your editor and LSP client, and the kamailio-lsp and Kamailio versions; remove real
addresses, domains and credentials from the configuration first. The
"Feature request" form asks what problem the change would solve. Write
issues in English. Every report is public and searchable at
<https://github.com/NormB/kamailio-lsp/issues>.

Do not report a vulnerability in a public issue. Use GitHub's private
vulnerability reporting at
<https://github.com/NormB/kamailio-lsp/security/advisories/new>;
[SECURITY.md](SECURITY.md) says what is in scope and how soon a report
is answered.
