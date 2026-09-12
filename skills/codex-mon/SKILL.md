---
name: cxi
description: "Execute tasks, prompts, subagents, and multi-account workflows using OpenAI Codex CLI (cxi / codex). Use whenever you need to inspect account quotas (cxi status), switch active accounts (cxi switch), run unattended Codex CLI sessions with automatic 5h sprint limit rotation, or manage ChatGPT Desktop App credentials."
argument-hint: "<prompt or cxi command>"
allowed-tools: ["Bash"]
metadata:
  short-description: "Run OpenAI Codex CLI with smart multi-account quota switching"
---

# OpenAI Codex CLI (`cxi`) Skill

Use `cxi` as the unified execution boundary for OpenAI Codex CLI (`codex`). `cxi` is a high-performance, quota-aware multi-account switcher and monitor that manages quotas across multiple OpenAI accounts using the **Reset-First** algorithm, automatically switching sessions before quota reaches zero (at ≤ 0% or configured threshold).

Binary path: `~/.local/bin/cxi` (or `~/.local/bin/codex-mon`).

---

## ⚡ Core Rules for AI Agents

1. **Check Quotas Before Long Runs**:
   ```bash
   cxi status
   ```
   Inspects the 5-hour rolling sprint window (`primary_window: 18000s`), 7-day rolling window (`secondary_window`), and reset credits (`rate_limit_reset_credits`).

2. **Wrap Long Unattended Commands**:
   ```bash
   cxi wrap codex exec "<prompt>"
   ```
   Performs pre-flight quota check and automatically rotates account if primary limit is exhausted.

3. **Switch Accounts on Demand**:
   ```bash
   cxi switch <account-id>
   ```
   Atomically swaps `~/.codex/auth.json` with POSIX `0600` permissions and cross-process file locks (`fs2` flock).

4. **Desktop App Sync & Automated Thread Recovery**:
   Both Codex CLI and `/Applications/ChatGPT.app` share `~/.codex/auth.json`. When switching accounts via `cxi switch`, active worker threads and turns paused by rate limits within the last 4 hours (`RECENT_QUOTA_WINDOW_SECS = 14400s`) across the top 30 unarchived threads are automatically resumed and cycled in the UI.

5. **Resume Interrupted or Rate-Limited Threads**:
   ```bash
   cxi resume [thread-id]
   ```
   Unpauses and queues a continuation message to the specified thread (or latest active/paused thread) and triggers native macOS Accessibility unpause.

---

## 🚀 Quick Command Reference

```bash
# 1. Show status of all accounts
cxi status

# 2. Switch to specific account (auto-resumes active & quota-paused threads)
cxi switch acc-2

# 3. Interactive setup wizard
cxi setup

# 4. Save current session from ~/.codex/auth.json
cxi save-current main

# 5. Remove account
cxi remove acc-2

# 6. Test API connectivity
cxi test

# 7. Install CLI shim (~/.local/bin/codex)
cxi install-shim

# 8. Start background monitoring daemon
cxi daemon

# 9. Resume most recent active or rate-limited thread
cxi resume

# 10. Resume specific thread by ID or URL
cxi resume 01a07d3c-3008-75c2-87a6-2c5c75f0e48b
```
