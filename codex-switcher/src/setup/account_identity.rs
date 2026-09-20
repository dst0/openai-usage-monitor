use crate::models::{AccountConfig, AuthTokens};

/// Builds a predictable, deterministic account ID from email and ChatGPT account_id (workspace UUID).
/// Example: "user@example.com:3f533057-4bac-44ea-a999-5bb5748ca9cb"
pub fn build_predictable_account_id(email: &str, account_id: &str) -> String {
    let email_clean = email.trim().to_lowercase();
    let acc_id_clean = account_id.trim();
    if acc_id_clean.is_empty() || acc_id_clean.eq_ignore_ascii_case("default") {
        format!("{}:default", email_clean)
    } else {
        format!("{}:{}", email_clean, acc_id_clean)
    }
}

/// Helper to identify whether an existing account matches by:
/// 1. Exact canonical ID or alias match
/// 2. Exact nickname match
/// 3. Exact refresh_token or access_token match
/// 4. Canonical predictable ID match
/// 5. Exact ChatGPT account_id match (if non-empty and != "default")
/// 6. Case-insensitive email match (if non-empty and unambiguous)
pub fn find_existing_account_idx_from_parts(
    accounts: &[AccountConfig],
    id: &str,
    email: &str,
    account_id: &str,
    plan: Option<&str>,
    refresh_token: Option<&str>,
    access_token: Option<&str>,
) -> Option<usize> {
    let email_trimmed = email.trim();
    let is_valid_email = |e: &str| -> bool {
        let t = e.trim();
        !t.is_empty()
            && t.contains('@')
            && !t.eq_ignore_ascii_case("user@openai.com")
            && !t.eq_ignore_ascii_case("current-user")
    };
    let has_real_email = is_valid_email(email_trimmed);
    let id_trimmed = id.trim();
    let acc_id_trimmed = account_id.trim();
    let has_acc_id = !acc_id_trimmed.is_empty() && acc_id_trimmed != "default";
    let rt_trimmed = refresh_token.map(str::trim).unwrap_or("");
    let at_trimmed = access_token.map(str::trim).unwrap_or("");
    let incoming_plan = plan.map(str::trim).unwrap_or("");

    // Helper: check if incoming email and candidate account email are in direct conflict
    let has_email_conflict = |candidate: &AccountConfig| -> bool {
        let cand_email = candidate.email.trim();
        let cand_has_real = is_valid_email(cand_email);
        has_real_email && cand_has_real && !cand_email.eq_ignore_ascii_case(email_trimmed)
    };

    // Helper: check if candidate has a known workspace ID that conflicts with incoming workspace ID
    let has_workspace_conflict = |candidate: &AccountConfig| -> bool {
        if !has_acc_id {
            return false;
        }
        let cand_tok_acc_id = candidate
            .tokens
            .account_id
            .as_deref()
            .map(str::trim)
            .unwrap_or("");
        let cand_acc_id = if !cand_tok_acc_id.is_empty() && cand_tok_acc_id != "default" {
            cand_tok_acc_id
        } else {
            candidate.account_id.trim()
        };
        let cand_has_acc_id = !cand_acc_id.is_empty() && cand_acc_id != "default";
        cand_has_acc_id && cand_acc_id != acc_id_trimmed
    };

    // Helper: check if candidate has a known plan that conflicts with incoming plan
    let has_plan_conflict = |candidate: &AccountConfig| -> bool {
        let cand_plan = candidate.plan_type.trim();
        !incoming_plan.is_empty()
            && !cand_plan.is_empty()
            && !cand_plan.eq_ignore_ascii_case(incoming_plan)
    };

    // Helper: check if candidate has a distinct ID that would be clobbered
    let has_id_conflict = |candidate: &AccountConfig| -> bool {
        if id_trimmed.is_empty() || candidate.id.trim().is_empty() {
            return false;
        }
        // "main" is allowed to be renamed to a descriptive name
        if candidate.id.trim().eq_ignore_ascii_case("main") {
            return false;
        }
        let cand_id = candidate.id.trim();
        let cand_name = candidate.name.as_deref().map(str::trim).unwrap_or("");
        !cand_id.eq_ignore_ascii_case(id_trimmed) && !cand_name.eq_ignore_ascii_case(id_trimmed)
    };

    // --- CASE 1: Explicit ID or Nickname provided by caller/user ---
    if !id_trimmed.is_empty() {
        // 1. Exact ID match
        if let Some(pos) = accounts.iter().position(|a| {
            if has_email_conflict(a) {
                return false;
            }
            a.id.trim().eq_ignore_ascii_case(id_trimmed)
        }) {
            return Some(pos);
        }

        // 2. Exact nickname match
        if let Some(pos) = accounts.iter().position(|a| {
            if has_email_conflict(a) || has_workspace_conflict(a) || has_plan_conflict(a) {
                return false;
            }
            a.name
                .as_deref()
                .map(|n| n.trim().eq_ignore_ascii_case(id_trimmed))
                == Some(true)
        }) {
            return Some(pos);
        }

        // 3. Same email and workspace UUID match (or canonical ID match)
        if has_real_email && has_acc_id {
            let canonical_id = build_predictable_account_id(email_trimmed, acc_id_trimmed);
            if let Some(pos) = accounts.iter().position(|a| {
                if has_email_conflict(a) || has_workspace_conflict(a) || has_plan_conflict(a) {
                    return false;
                }
                a.id.trim().eq_ignore_ascii_case(&canonical_id)
                    || (a.email.trim().eq_ignore_ascii_case(email_trimmed)
                        && (a.account_id.trim() == acc_id_trimmed
                            || a.tokens.account_id.as_deref().map(str::trim)
                                == Some(acc_id_trimmed)))
            }) {
                return Some(pos);
            }
        }

        // 4. Token match (same session credentials being updated)
        if !rt_trimmed.is_empty() {
            if let Some(pos) = accounts
                .iter()
                .position(|a| a.tokens.refresh_token.as_deref().map(str::trim) == Some(rt_trimmed))
            {
                return Some(pos);
            }
        }
        if !at_trimmed.is_empty() {
            if let Some(pos) = accounts
                .iter()
                .position(|a| a.tokens.access_token.trim() == at_trimmed)
            {
                return Some(pos);
            }
        }

        return None;
    }

    // --- CASE 2: No explicit ID provided (auto-sync / deduplication) ---

    // Tier 1: Refresh token match across all accounts
    if !rt_trimmed.is_empty() {
        if let Some(pos) = accounts
            .iter()
            .position(|a| a.tokens.refresh_token.as_deref().map(str::trim) == Some(rt_trimmed))
        {
            return Some(pos);
        }
    }

    // Tier 2: Access token match across all accounts
    if !at_trimmed.is_empty() {
        if let Some(pos) = accounts
            .iter()
            .position(|a| a.tokens.access_token.trim() == at_trimmed)
        {
            return Some(pos);
        }
    }

    // Tier 2.5: Canonical predictable ID match (email + workspace UUID)
    if has_real_email && has_acc_id {
        let canonical_id = build_predictable_account_id(email_trimmed, acc_id_trimmed);
        if let Some(pos) = accounts
            .iter()
            .position(|a| a.id.trim().eq_ignore_ascii_case(&canonical_id))
        {
            return Some(pos);
        }
    }

    // Tier 3: ChatGPT Account ID (Workspace UUID) match
    if has_acc_id {
        let matching_candidates: Vec<usize> = accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                if has_email_conflict(a)
                    || has_workspace_conflict(a)
                    || has_plan_conflict(a)
                    || has_id_conflict(a)
                {
                    return false;
                }
                a.account_id.trim() == acc_id_trimmed
                    || a.tokens.account_id.as_deref().map(str::trim) == Some(acc_id_trimmed)
            })
            .map(|(idx, _)| idx)
            .collect();

        if matching_candidates.len() == 1 {
            return Some(matching_candidates[0]);
        }
    }

    // Tier 4: Unambiguous email match
    if has_real_email {
        let email_candidates: Vec<usize> = accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                if has_workspace_conflict(a) || has_plan_conflict(a) || has_id_conflict(a) {
                    return false;
                }
                a.email.trim().eq_ignore_ascii_case(email_trimmed)
            })
            .map(|(idx, _)| idx)
            .collect();

        if email_candidates.len() == 1 {
            return Some(email_candidates[0]);
        }
    }

    None
}

pub fn find_existing_account_idx(
    accounts: &[AccountConfig],
    id: &str,
    email: &str,
    account_id: &str,
    plan: Option<&str>,
    tokens: Option<&AuthTokens>,
) -> Option<usize> {
    let effective_acc_id = if !account_id.trim().is_empty() && account_id.trim() != "default" {
        account_id
    } else if let Some(t_acc) = tokens.and_then(|t| t.account_id.as_deref()) {
        t_acc
    } else {
        account_id
    };

    let mut resolved_plan = plan;
    let extracted_plan;
    if resolved_plan.is_none() {
        if let Some(t) = tokens {
            extracted_plan = crate::oauth::extract_jwt_metadata_from_tokens(t).1;
            resolved_plan = extracted_plan.as_deref();
        }
    }

    find_existing_account_idx_from_parts(
        accounts,
        id,
        email,
        effective_acc_id,
        resolved_plan,
        tokens.and_then(|t| t.refresh_token.as_deref()),
        tokens.map(|t| t.access_token.as_str()),
    )
}
