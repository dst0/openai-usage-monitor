## 📝 Pull Request Summary

Provide a concise overview of what this pull request changes and why.

### 🔗 Related Issue
Fixes # (issue number if applicable)

---

## 🔍 Type of Change
- [ ] 🐛 Bug fix (non-breaking change fixing an issue)
- [ ] ✨ New feature (non-breaking enhancement)
- [ ] ⚡ Performance optimization
- [ ] 📚 Documentation update
- [ ] 🧪 Tests / CI improvement

---

## 🛡️ Pre-Merge Verification Checklist

Every pull request must fulfill these conditions before being reviewed by the maintainer:

- [ ] **Tests pass locally**:
  - `cd codex-switcher && cargo test --locked` (Rust CLI test suite against the committed `Cargo.lock`)
  - `./scripts/test_swift.sh` (Swift Menu Bar test suite)
- [ ] **Security & Privacy**:
  - No access tokens, refresh tokens, API keys, or private paths are committed.
  - POSIX `0600` permissions are strictly preserved on all auth files.
- [ ] **Zero Overhead & Parity**:
  - Architecture keeps memory overhead < 5 MB RAM.
  - Thread safety and atomic file locks (`flock`) are preserved.

> [!NOTE]
> All Pull Requests require final review and merge by repository owner **@dst0**. External merges are blocked by branch protection policies.
