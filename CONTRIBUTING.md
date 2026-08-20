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

```sh
cargo test                     # full suite, includes the stdio LSP e2e
cargo clippy --all-targets     # CI enforces -D warnings
cargo fmt
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps   # missing_docs is deny
```

The real-tree harvest test is opt-in:
`KAMAILIO_LSP_TEST_TREE=/path/to/kamailio cargo test`.

## PRs

- Feature branches, one problem per PR, linear history (squash or
  rebase merges).
- `docs/ADMIN.md` documents every user-facing option and its
  structure is enforced by a test — update it when options change.
- The VS Code client (`client/`) compiles in CI with `tsc`; keep its
  settings in sync with the server's initialization options.
