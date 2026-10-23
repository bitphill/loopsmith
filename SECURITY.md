# Security policy

## Reporting a vulnerability

Please report security problems privately, not in a public issue or Discussion.

Use GitHub's private vulnerability reporting:
[**Report a vulnerability**](https://github.com/bitphill/loopsmith/security/advisories/new)
(the *Security* tab, then *Report a vulnerability*).

Include what you found, the version (`loopsmith --version`), the operating system,
and the smallest steps that reproduce it. If the report involves a config file,
please strip any secrets from it first.

This is a small project, so there is no service-level promise. Reports are read, and a reply is the goal, but it may take some days.

## What counts

loopsmith runs commands you configure, so the useful line is between what the
tool is *meant* to let a loop do and what it should never allow. Examples worth
reporting:

- a way for a loop to escape its declared permission grant or its worktree
- a secret reaching a prompt, a log or the ledger (loopsmith is meant to check
  only that `requires_env` variables exist, and never read their values)
- the browser UI (`loopsmith --web`) accepting connections from anything other
  than `127.0.0.1`, or performing an action `loopsmith --help` does not list
- an installer, npm or PyPI wrapper that fetches or runs something other than the
  release it claims to

A loop that does what its own config told it to do is not a vulnerability.

## Supported versions

Only the latest release gets fixes.

## Credentials

If you find a leaked credential, report it the same way, and rotate it.
