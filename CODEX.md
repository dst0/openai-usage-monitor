# OpenAI Codex Configuration & Integration

- Project: OpenAI Codex Usage Monitor & Account Switcher
- CLI Commands: `cxi` / `codex-mon`
- CLI Wrapper / Shim: `~/.local/bin/codex` -> `cxi wrap`
- Quota API Endpoint: `https://chatgpt.com/backend-api/wham/usage`
- OAuth Refresh Endpoint: `https://auth.openai.com/oauth/token` (Client ID: `app_EMoamEEZ73f0CkXaXp7hrann`)
- Native App: `/Applications/Codex Monitor.app`

## Runtime Paths & Files
- `~/.codex/auth.json`: Active authentication tokens used by Codex CLI and `ChatGPT.app` (0600 permissions).
- `~/.codex/accounts.json`: Configured accounts database, multipliers, and cached quota metrics (0600 permissions).
- `~/.codex/usage-status.json`: Real-time quota snapshot consumed by the macOS Menu Bar app.
- `~/.codex/thread-writer-locks/`: Active flock files held by Codex worker threads.
- `~/.codex/state_5.sqlite`: Thread metadata database used by the thread detection engine.

## Core CLI Commands
- `cxi status`: Check quota table across all accounts (`5H SPRINT`, `7D LIMIT`, `PLAN (MULT)`, `CREDITS`).
- `cxi switch <account>`: Switch active account (automatically detects and resumes active/interrupted turns).
- `cxi resume [thread-id]`: Resume a paused or rate-limited thread via Codex queue socket and AXUIElement.
- `cxi config`: Inspect and configure auto-switch modes (`--auto-switch-enabled`, `--auto-switch-business-only`, `--auto-switch-business-priority`, `--restart-app-on-switch`).
- `cxi set-multiplier <account> <val>`: Set custom quota multiplier override (e.g. 20 for Pro 20x).
- `cxi reset-multiplier <account>`: Reset multiplier back to auto-detected default.
- `cxi wrap exec "<prompt>"`: Run unattended command with pre-flight quota check and auto-switch.

