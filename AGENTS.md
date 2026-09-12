# Codex Switcher & Monitor — Agent Guidance

## Core Architecture
- `codex-switcher/`: Rust CLI (`cxi`, `codex-mon`) for quota monitoring and atomic account switching.
- `Sources/`: Swift AppKit Menu Bar application (`Codex Monitor.app`).
- `~/.codex/auth.json`: Active credentials used by Codex CLI and ChatGPT.app.
- `~/.codex/accounts.json`: Multi-account credentials and quota cache (POSIX 0600 permissions).
- `~/.codex/usage-status.json`: Cache for Swift status bar app.

## Invariants
- POSIX `0600` permissions on all credential and token files.
- Never print or log tokens/secrets to stdout/stderr.
- Always use atomic file operations (`fs2` flock) when writing `auth.json` or `accounts.json`.
- Keep binary overhead under 5 MB RAM.
