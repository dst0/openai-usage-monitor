# 2026-09-26 — CI cargo cache key hashed an ignored lockfile

- **Status:** Partial
- **Task/context:** Pin the Rust toolchain and harden `.github/workflows/ci.yml`.
- **Unexpected observation or failure:** The cache key `hashFiles('codex-switcher/Cargo.lock')` never changed because `.gitignore` excludes `*.lock`, so no `Cargo.lock` exists in a clean checkout.
- **Evidence:** `hashFiles` returns an empty string for missing files, so every run used the constant key `macOS-cargo-`. The last `main` run logged a hit on that key and skipped saving, then recompiled `clap` crates anyway: `actions/cache` never overwrites an existing key, so the first saved `target/` was restored indefinitely, including across rustc upgrades. The cache also saved `~/.cargo/bin/`, which on hosted runners holds the rustup proxies.
- **Approaches tried:**
  - **Attempt:** Key on `rust-toolchain.toml` and `Cargo.toml`.
    - **Outcome:** Partial
    - **Why:** Toolchain bumps get a fresh key, but floating dependency resolutions within one `Cargo.toml` still reuse a stale cache.
  - **Attempt:** Run `cargo generate-lockfile` before the cache step, key on the toolchain file plus the generated `Cargo.lock`, restore only within the same toolchain hash, and stop caching `~/.cargo/bin/`.
    - **Outcome:** Worked
    - **Why:** A toolchain bump or any change in resolved dependencies produces a new key; artifacts from another compiler are never restored; cached rustup binaries cannot replace the runner's.
- **Root cause:** A cache key derived from a file the repository deliberately does not track.
- **Resolution:** New resolve step, key, and restore prefix in `.github/workflows/ci.yml`.
- **Verification:** Pending: the PR's first CI run must show a miss on the new key, a save, and a green Rust job. Update this entry with the run ID once observed.
- **Prevention/follow-up:** Consider committing `Cargo.lock` for the `codex-mon` binary so dependency resolution is reproducible; that is a separate supply-chain change.
- **Reusable learning:** Check that every `hashFiles` input exists in a clean checkout; a missing file silently produces a constant cache key.
- **References:** `.github/workflows/ci.yml`, `.gitignore`.
