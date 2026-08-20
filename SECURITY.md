# Security Policy

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

Report privately via GitHub's private vulnerability reporting:
<https://github.com/NormB/kamailio-lsp/security/advisories/new>

Include a description, steps to reproduce or proof of concept, an
impact assessment, and (optionally) your name/handle for credit.

## Response Timeline

| Stage | Target |
|-------|--------|
| Acknowledgment | 48 hours |
| Initial assessment | 7 days |
| Fix for critical issues | 30 days |
| Public disclosure | After fix is released |

## Scope

- **The `-C` execution model** — the server runs `kamailio -c` on the
  opened file, which dlopens the modules that file loads. Bypasses of
  the documented opt-out (`kamailioPath` empty), or ways to make the
  server run `-C` on files/paths the user did not open, are in scope.
- **Parser robustness** — crafted cfg text, README/markdown module
  docs, or `kamailio -c` output that crashes the server or corrupts
  its responses (all three parsers are fuzz-adjacent surfaces; they
  must fail closed).
- **Subprocess handling** — argument injection into the `kamailio`
  invocation, or resource exhaustion that survives the timeout and
  serialization bounds.

## Out of scope

- Vulnerabilities in Kamailio itself (report to the Kamailio project).
- Code execution caused by opening an untrusted cfg *with diagnostics
  deliberately enabled* — that is the documented trust model; see the
  Security section of `docs/ADMIN.md`.
