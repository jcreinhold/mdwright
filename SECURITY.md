# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 0.3.x   | yes       |
| < 0.3   | no        |

## Reporting a Vulnerability

`mdwright` is built to be safe on untrusted Markdown input. The following
classes of bug qualify as security issues:

- **Panics** on any UTF-8 input (parse, lint, or format paths).
- **Unbounded memory** — inputs that allocate proportionally to anything
  other than `O(input length)`.
- **Hangs or non-termination** — inputs that fail to complete in a time
  budget linear in their size.
- **Out-of-tree filesystem access** — `mdwright fmt` writing to or reading
  from files outside the user-supplied roots.

Please report privately via email to **jcreinhold@gmail.com** with:

- A minimal reproducing input (bytes, hex, or attached file).
- The command line invoked.
- The observed and expected behaviour.
- The version of mdwright (`mdwright --version`).

I aim to acknowledge reports within seven days. For fuzz-generated
inputs you can also open a regular GitHub issue with the
`security:fuzz` label if the impact is limited to a denial-of-service
or formatter quirk rather than a memory-safety bug.

## Out of scope

- Formatter idempotence or HTML-equivalence regressions on
  well-formed Markdown that does not crash mdwright. Please file these
  as regular bugs.
- Behaviour when the `--max-input-bytes 0` (uncapped) escape hatch is
  used; that flag is opt-in and removes the size guarantee.
- Issues that require write access to the host (e.g., a malicious
  configuration file): mdwright trusts its config file by design.
