# Security Policy

## Reporting a Vulnerability

If you believe you've found a security issue in `bigfiles`, please report it privately.

**Preferred:** open a [private security advisory on GitHub](https://github.com/Par-python/bigfiles/security/advisories/new). This keeps the report confidential until a fix ships.

**Email:** `pardojeromeimportant@gmail.com` with subject `[bigfiles security]`.

Please include:

- A description of the issue and its impact
- Steps to reproduce (or a proof-of-concept if you have one)
- The version of bigfiles you tested against (`bigfiles --version`)
- Your platform (OS, architecture)

You should receive an initial response within **7 days**. If you don't, please follow up — your report may have been missed.

## Supported Versions

Only the latest minor version line receives security fixes. Once `1.x` is current, `0.x` versions are unsupported.

| Version | Supported |
| --- | --- |
| 1.x | yes |
| < 1.0 | no |

## Scope

In-scope:

- File deletion behavior (`bigfiles delete`, `bigfiles dupes --delete`)
- Path traversal or unexpected file access during scanning
- Argument parsing leading to unintended filesystem operations
- TOCTOU / race conditions in the deletion flow

Out of scope:

- Behavior caused by malicious local files (e.g. crafted symlinks) when `--no-ignore` is set and the user has explicitly opted in to following them
- Performance issues that don't lead to incorrect deletion or data loss
- Output rendering bugs (cosmetic)
