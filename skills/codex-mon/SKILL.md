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
   Both Codex CLI and `/Applications/ChatGPT.app` share `~/.codex/auth.json`. When switching accounts via `cxi switch`, eligible restart-captured turns and turns paused by rate limits within the last 4 hours (`RECENT_QUOTA_WINDOW_SECS = 14400s`) across the top 30 unarchived user threads are offered to the standard Desktop owner's same-user IPC connection. A cold task is opened by URL, with a bounded native retry, but IPC is sent only after Desktop reports a real owner. Automatic switching remains disabled until installed end-to-end recovery and exact selected-task restoration across multiple windows are verified. Ambiguous active turns are not guessed at in discovery-only mode. Desktop's bundled app-server remains the only thread writer; no second/headless app-server is started.

5. **Resume Interrupted or Rate-Limited Threads**:
   ```bash
   cxi resume [thread-id]
   ```
   Sends one owner-routed `thread-follower-start-turn` request with the protocol-valid text `continue` (or restores an existing queue) through Desktop IPC after task ownership is confirmed. URL acceptance alone is not recovery proof, and ChatGPT may come to the foreground while mounting a cold task. Accessibility is used only for the recovery banner and final visibility check, not to click Play/Resume/Retry/Steer controls. With an explicit ID it also resumes a turn that ended with a non-quota error (for example an outage 401) before a final agent message, claims a stale deferred owner-wait for that task, and starts ChatGPT in the background if it is closed.

6. **Manage Auto-Switching Policies & Multipliers**:
   ```bash
   # Restrict auto-switching to corporate/business accounts:
   cxi config --auto-switch-business-only true

   # Prioritize business accounts with preemptive return on quota recovery:
   cxi config --auto-switch-business-priority true

   # Override plan multiplier (e.g. 20 for Pro 20x, 5 for Business Premium):
   cxi set-multiplier <account> <multiplier>
   ```

---

## 🚀 Quick Command Reference

```bash
# 1. Show status of all accounts (displays PLAN (MULT) and 5H SPRINT (EQ))
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

# 9. Resume the most recent eligible quota-blocked or restart-captured thread
cxi resume

# 10. Resume specific thread by ID or URL
cxi resume 01a07d3c-3008-75c2-87a6-2c5c75f0e48b

# 11. Configure auto-switch modes and app restart
cxi config --auto-switch-enabled true
cxi config --auto-switch-business-only true
cxi config --auto-switch-business-priority true
cxi config --restart-app-on-switch true

# 12. Set or reset custom quota multiplier
cxi set-multiplier main 20
cxi reset-multiplier main
```
