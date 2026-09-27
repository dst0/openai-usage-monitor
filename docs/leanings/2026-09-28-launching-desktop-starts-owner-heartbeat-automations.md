# 2026-09-28 — Launching Desktop for a live test would start the owner's heartbeat automations

- **Status:** Open
- **Task/context:** Live verification of multiwindow task restoration on the owner's machine, under rules that allow quitting and relaunching ChatGPT but forbid starting, continuing, or stopping any Codex turn.
- **Unexpected observation or failure:** ChatGPT Desktop was not running when the work began (its last main process had exited, leaving helper processes behind), so any live UI check first required launching it. The owner has active heartbeat automations with 10- and 15-minute schedules, and Desktop runs heartbeat automations only while it is running.
- **Evidence:** Read-only checks on 2026-09-28: no ChatGPT main process in the process table (LaunchServices listed the last one as exited); no held writer locks and no rollout written in the previous three hours; `~/.codex/automations/*/automation.toml` status and schedule fields only (no prompts read) showed several `ACTIVE` heartbeats, two of them every 10 and 15 minutes. The installed Desktop's scheduler starts a heartbeat run as a turn in its target thread once the thread and renderer state allow it; there is no environment switch that disables automations for one launch.
- **Approaches tried:**
  - **Attempt:** Launch ChatGPT with `open -g -a /Applications/ChatGPT.app`, run the checks, and quit it again.
    - **Outcome:** Not attempted.
    - **Why:** The launch would let Desktop start automation turns, which the live-test rules forbid, and a running heartbeat would then block the required graceful quit until it ends.
  - **Attempt:** A second, isolated Desktop instance with its own user-data and Codex home.
    - **Outcome:** Rejected.
    - **Why:** It would show no tasks without the owner's credentials, and copying credentials is not allowed.
- **Root cause:** Live Desktop checks share the machine with the owner's scheduled automations; launching Desktop is not side-effect free.
- **Resolution:** Open. No live Desktop check was run. The restore path ships behind an explicit flag with a rehearsal command the owner can run on their own open windows.
- **Verification:** Not applicable; the blocking state was verified read-only as described.
- **Prevention/follow-up:** Before a live Desktop test, check whether ChatGPT is already running and which automations are active. Run the rehearsal and any restart only while the owner has Desktop open, no turn is running, and a restart cannot interrupt a due heartbeat (or the owner has paused it).
- **Reusable learning:** Opening an app for a test can trigger the owner's scheduled work; check for schedulers before launching, not after.
- **References:** `2026-09-28-desktop-restores-one-window-and-links-target-last-focused.md`, `README.md` (`--restore-window-tasks`, `rehearse-task-restore`).
