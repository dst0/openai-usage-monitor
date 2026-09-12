#!/bin/bash
set -euo pipefail

# ==============================================================================
# 🛡️ Configure GitHub Branch Protection & Anti-Spam Policies
# ==============================================================================

REPO="dst0/openai-usage-monitor"

echo "🔍 Checking GitHub CLI authentication..."
if ! command -v gh >/dev/null 2>&1; then
    echo "❌ GitHub CLI (gh) not found. Please install via: brew install gh"
    exit 1
fi

echo "🔒 Applying Branch Protection Rules for 'main' on ${REPO}..."
gh api -X PUT "repos/${REPO}/branches/main/protection" \
    --input - << JSON
{
  "required_status_checks": {
    "strict": true,
    "contexts": [
      "🦀 Rust Core Tests (cxi / codex-mon)",
      "🍏 Swift Menu Bar App Tests",
      "🔒 Security & Privacy Scan"
    ]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": true,
    "require_code_owner_reviews": true,
    "required_approving_review_count": 1,
    "require_last_push_approval": true
  },
  "restrictions": {
    "users": ["dst0"],
    "teams": [],
    "apps": []
  },
  "required_linear_history": false,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "block_creations": false,
  "required_conversation_resolution": true,
  "lock_branch": false,
  "allow_fork_syncing": true
}
JSON

echo "✨ Branch protection successfully applied!"
echo "   - Direct push to 'main' is restricted exclusively to @dst0."
echo "   - Pull Requests require passing CI checks and approval from @dst0."
echo "   - Force pushes and branch deletion are blocked."
