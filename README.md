# 🚀 OpenAI Codex Multi-Account Monitor & Switcher

Ultra-lightweight, high-performance automatic quota monitoring and account rotation system for **OpenAI Codex** (supporting both console **Codex CLI** and desktop **Codex / ChatGPT.app**).

Engineered with **100% functional parity** and zero-overhead performance: core in **Rust** (~3 MB RAM footprint, instantaneous execution) paired with a native macOS Menu Bar status application in **Swift** (`Codex Monitor.app`).

---

## 🎯 Key Features

1. **5-Hour Rolling Sprint Monitoring (`wham/usage`)**:
   - Precise quota calculation (`100% - used_percent`) for the primary 5-hour rolling window (`limit_window_seconds: 18000`).
   - Exact countdown timer until rate-limit reset (e.g. `↻ 4h 45m`).
   - Tracking of the 7-day secondary limit window (`secondary_window: 604800s`) and rate-limit reset credits (`rate_limit_reset_credits`).

2. **Automatic Account Switching on Limit Exhaustion (or Custom Threshold)**:
   - When the active account exhausts its quota, the background daemon automatically selects the optimal account using the **Reset-First / Highest-Quota** algorithm.
   - Atomically updates `~/.codex/auth.json` protected by `fs2` file locks (`flock`) and strict POSIX `0600` permissions.

3. **Instant Switching for Codex CLI**:
   - Codex CLI reads `~/.codex/auth.json` on each invocation.
   - Transparent `cxi` / `codex-mon` shim or `codex` wrapper ensures agent swarms and terminal sessions never fail with `429 Rate Limit Exceeded`.

4. **Desktop Application Switching (`ChatGPT.app`)**:
   - The desktop app (`/Applications/ChatGPT.app`, bundle ID `com.openai.codex`) shares the `~/.codex/auth.json` credentials.
   - When an account is switched, the tool gracefully restarts the desktop app (`restart_app_on_switch: true`), immediately updating the interface and active sessions to the new account.

5. **Native macOS Menu Bar App (`Codex Monitor.app`)**:
   - Official Codex icon in the status bar.
   - Dual-session live display: `APP 97% | CLI 97% (↻ 4h 45m)`.
   - Distinctive 3D shield badges `[ 🛡️ ] 🛡️ 🛡️` with a 3-tier visual gauge (Top = 5h sprint, Center = 7-day pool, Bottom = reset credits).
   - Rich dropdown menu:
     - **Block 1**: 🖥️ Codex Desktop App (`ChatGPT.app`) — active account, status `[ACTIVE IN APP]`, sprint and weekly progress bars, credit balance.
     - **Block 2**: 💻 Codex CLI — active account, status `[ACTIVE IN CLI]`, remove button `✕`, progress bars, CLI model selection submenu (`gpt-5-5`, `gpt-5-4`, `o3`, `gpt-4.5`).
     - **Block 3**: 👥 Backup Accounts — 1-click instant switch buttons, individual progress bars, account removal.
   - Quick action `➕ Add Account via Terminal...`.
   - Configurable polling interval (1m, 5m, 15m, 30m) persisted in `UserDefaults`.
   - Desktop app restart button.
   - Built-in offline documentation with interactive menu bar simulator (`helps.html`).
   - Multilingual documentation support (EN, UK, DE, FR, ES, IT, PT, PL, NL).
   - Launch at Login support (`Launch at Login`).

---

## 🛠 Installation & Setup

Run the automated installation script:

```bash
./scripts/install.sh
```

The script automatically:
1. Compiles the release Rust binary `codex-mon` and installs it to `~/.local/bin/codex-mon`, creating a symlink at `~/.local/bin/cxi`.
2. Configures the transparent CLI shim at `~/.local/bin/codex`.
3. Builds the native Swift status bar application with all bundled resources (`build/Codex Monitor.app`).
4. Ad-hoc codesigns the app and installs it to `/Applications/Codex Monitor.app`.
5. Configures macOS Login Items for automatic launch at login.
6. Connects Skills for Claude Code, Codex, and agent swarms in `~/.codex/skills` and `~/.claude/skills`.
7. Launches the application.

### Background Daemon (`launchd`)
To keep automatic quota monitoring and account rotation active 24/7 in the background:
```bash
sed "s|__HOME__|${HOME}|g" com.codex.switcher.plist > ~/Library/LaunchAgents/com.codex.switcher.plist
launchctl load ~/Library/LaunchAgents/com.codex.switcher.plist
```

---

## 📖 CLI Commands (`cxi` / `codex-mon`)

### 1. Check Current Status and Quotas
```bash
cxi status
# Or force a live refresh from OpenAI API:
cxi status --refresh
```

Example output:
```text
📊 OpenAI Codex Accounts & Rate Limits (Strategy: reset-first)
--------------------------------------------------------------------------------------------------
     ACCOUNT      EMAIL                        PLAN     5H SPRINT          RESET IN     7D LIMIT   CREDITS
--------------------------------------------------------------------------------------------------
→ 🟢  main         dev@example.com              team     ████████  97%      4h 45m       68%        3      
  🟢  backup       backup@example.com           plus     ████████ 100%      now         100%        0      
--------------------------------------------------------------------------------------------------
💡 Switch account: `codex-mon switch <id>` | Run daemon: `codex-mon daemon`
```

### 2. Save Current Authenticated Session
```bash
cxi save-current main
```

### 3. Interactive Account Setup Wizard
```bash
cxi setup
```
Allows you to:
- Save your current session under any label (`main`, `work`, `team-1`).
- Initiate browser authentication via `codex login` to register additional accounts.
- Inspect quota status across all configured accounts.

### 4. Add Account via Browser Login
```bash
cxi add secondary
```

### 5. Manual Account Switching
```bash
cxi switch backup
# Without restarting the Desktop App:
cxi switch backup --no-restart
```

### 6. Rename / Label an Account
```bash
cxi rename backup "Work Backup"
# Clear custom label:
cxi rename backup --clear
```

### 7. Configure Settings
```bash
# Enable/disable automatic ChatGPT.app restart on switch:
cxi config --restart-app-on-switch true
```

### 8. Remove an Account
```bash
cxi remove backup
```

### 9. Run Commands with Automatic Quota Check & Switch
```bash
cxi wrap exec "fix bug in auth"
```

### 10. Open Interactive Documentation
```bash
cxi helps
```

---

## 📁 Configuration Structure

All configuration files and runtime caches reside in `~/.codex/`:
- `~/.codex/auth.json` — Active tokens used by Codex CLI and ChatGPT.app (strict `0600` permissions).
- `~/.codex/accounts.json` — Stored multi-account credentials and cached quotas (strict `0600` permissions).
- `~/.codex/usage-status.json` — Real-time quota snapshot consumed by the macOS Menu Bar app.
- `~/.codex/monitor.lock` — File lock preventing concurrent daemon instances.
- `~/.codex/helps.html` — Offline interactive documentation guide.
