# OpenAI Codex Configuration & Integration

- Project: OpenAI Codex Usage Monitor & Account Switcher
- CLI Commands: `cxi` / `codex-mon`
- CLI Wrapper / Shim: `~/.local/bin/codex` -> `cxi wrap`
- Quota API Endpoint: `https://chatgpt.com/backend-api/wham/usage`
- OAuth Refresh Endpoint: `https://auth.openai.com/oauth/token` (Client ID: `app_EMoamEEZ73f0CkXaXp7hrann`)
- Native App: `/Applications/Codex Monitor.app` when writable, otherwise `~/Applications/Codex Monitor.app` (the installer may also leave a symlink at the latter path).

## Official Codex Desktop Integration

The supported Desktop setup is the normal OpenAI Codex Desktop installation
(`/Applications/ChatGPT.app` in the standard macOS layout), signed in and
running normally. The installer does not patch `ChatGPT.app`, install a second
App Server, add custom App Server flags, or require manual IPC configuration.

ChatGPT Desktop starts its bundled `codex app-server`. The Monitor is only a
same-user IPC client: it connects to `~/.codex/ipc/ipc.sock`, discovers the
Desktop window that owns a task, and routes `thread-follower-start-turn` or
queued-follow-up requests to that owner. The Desktop-owned app-server remains
the only thread writer; the Monitor never runs `codex exec resume`.

Without Desktop, CLI quota inspection and account switching still work, but
Desktop restart and thread recovery are unavailable. Optional read-only check:
`cxi recovery-preflight`.

## Runtime Paths & Files
- `~/.codex/auth.json`: Active authentication tokens used by Codex CLI and `ChatGPT.app` (0600 permissions).
- `~/.codex/accounts.json`: Configured accounts database, multipliers, and cached quota metrics (0600 permissions).
- `~/.codex/usage-status.json`: Real-time quota snapshot consumed by the macOS Menu Bar app.
- `~/.codex/ipc/` and `~/.codex/app-server-daemon/`: Official Codex Desktop IPC and app-server runtime state (preserved by the Monitor uninstaller).
- `~/.codex/thread-writer-locks/`: Active flock files held by Codex worker threads.
- `~/.codex/state_5.sqlite`: Thread metadata database used by the thread detection engine.

## Core CLI Commands
- `cxi status`: Check quota table across all accounts (`5H SPRINT`, `7D LIMIT`, `PLAN (MULT)`, `CREDITS`).
- `cxi switch <account>`: Switch active account (automatically recovers eligible quota-blocked or restart-captured turns; ambiguous active turns are not dispatched by discovery-only recovery).
- `cxi resume [thread-id]`: Resume an eligible quota-blocked or restart-captured thread through the Desktop owner's same-user IPC channel. Accessibility is used only for recovery visibility/banner verification, not to dispatch the turn.
- `cxi config`: Inspect and configure auto-switch modes (`--auto-switch-enabled`, `--auto-switch-business-only`, `--auto-switch-business-priority`, `--restart-app-on-switch`).
- `cxi set-multiplier <account> <val>`: Set custom quota multiplier override (e.g. 20 for Pro 20x).
- `cxi reset-multiplier <account>`: Reset multiplier back to auto-detected default.
- `cxi wrap exec "<prompt>"`: Run unattended command with pre-flight quota check and auto-switch.

## Localization & Supported Languages
- Supported Languages (13 in the native app): English (`en`), Japanese (`ja`), Simplified Chinese (`zh-Hans`), Vietnamese (`vi`), Ukrainian (`uk`), German (`de`), French (`fr`), Spanish (`es`), Italian (`it`), Portuguese (`pt`), Polish (`pl`), Dutch (`nl`), Russian (`ru`). The offline `helps.html` guide supports the first 12; Russian is app-UI only.
- Native macOS Bundles: `resources/*.lproj/Localizable.strings` bundled inside `Codex Monitor.app/Contents/Resources/`.
- Offline Interactive Guide: `resources/helps.html` with language switcher and URL parameter routing (`?lang=ja`, `?lang=zh-Hans`, `?lang=vi`).

## Uninstall

Use `./scripts/uninstall.sh` from the checkout (the installer also bundles a
copy inside the Monitor app). It stops the Monitor and its
`launchd` workers, removes the Login Item, installed bundles, CLI/helper
shims, copied guide, app-owned logs/journals, app-specific support/cache/state
directories, preferences, and project skill links. It asks for confirmation; use `--yes` only for an explicitly approved
non-interactive run. Runtime status/recovery files are removed by the normal
uninstall; add `--purge-data` only to remove the Monitor-owned account registry
such as `~/.codex/accounts.json`.

During installation, log migration occurs only after the newly built app is
signed and after the exact Monitor app, daemon, and restart-worker launchd job
are verified stopped. The Rust helper redacts pre-existing active Monitor logs,
exact Monitor Brotli archives, and exact restart-worker logs with bounded
streaming and no-follow opens. It fails closed and preserves the source for
malformed, oversized, symlinked, or concurrently changed inputs; official
Codex Desktop logs and foreign archive entries are outside this scope.

The same operation is available in the running Menu Bar app: choose
`⛔ UNINSTALL CODEX MONITOR…` and type `UNINSTALL` in uppercase. The app then
starts its bundled uninstaller and exits. An unchecked option preserves
`accounts.json`; select it only when the Monitor's account registry should be
removed, or use the explicit `--purge-data` terminal option.

The uninstaller deliberately preserves official Codex Desktop and shared
state—`ChatGPT.app`, its bundled app-server, `~/.codex/auth.json`,
`state_5.sqlite`, `queue_1.sqlite`, sessions, thread locks, user `config.toml`,
and source checkouts. It cannot erase shell history, macOS unified logs,
notification history, LaunchServices caches, or TCC records; “no traces” means
no persistent Monitor installation artifacts, not forensic erasure of the OS.
