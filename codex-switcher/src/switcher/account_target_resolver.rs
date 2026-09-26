use crate::models::{AccountConfig, AccountsFile, AuthJson};

pub(crate) fn resolve_account_with_sync(
    accounts: &mut AccountsFile,
    query: &str,
    sync: impl FnOnce(&mut AccountsFile) -> Result<Option<AuthJson>, String>,
) -> Result<(AccountConfig, Option<AuthJson>), String> {
    let index = resolve_target_account_idx(&accounts.accounts, query)?;
    let selected_id = accounts.accounts[index].id.clone();
    let observed = sync(accounts)?;
    let mut matches = accounts
        .accounts
        .iter()
        .filter(|account| account.id.eq_ignore_ascii_case(&selected_id));
    let selected = matches
        .next()
        .cloned()
        .ok_or("Selected account disappeared during authentication sync")?;
    if matches.next().is_some() {
        return Err("Selected account became ambiguous during authentication sync".into());
    }
    Ok((selected, observed))
}

/// Resolves a user-provided account query to an account index.
/// Matching order:
/// 1. Exact canonical ID (<email>:<account_id>)
/// 2. Exact nickname (`name`)
/// 3. Exact ChatGPT workspace account_id UUID
/// 4. Unambiguous exact email
/// 5. Unambiguous prefix of canonical ID, UUID, or nickname (len >= 3)
pub fn resolve_target_account_idx(
    accounts: &[AccountConfig],
    query: &str,
) -> Result<usize, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Account identifier cannot be empty".to_string());
    }

    // 1. Exact canonical ID match
    if let Some(pos) = accounts.iter().position(|a| a.id.eq_ignore_ascii_case(q)) {
        return Ok(pos);
    }

    // 2. Exact nickname match (case-insensitive)
    let nick_matches: Vec<usize> = accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| a.name.as_deref().map(|n| n.trim().eq_ignore_ascii_case(q)) == Some(true))
        .map(|(i, _)| i)
        .collect();
    if nick_matches.len() == 1 {
        return Ok(nick_matches[0]);
    } else if nick_matches.len() > 1 {
        return Err(format!(
            "Multiple accounts share nickname '{}'. Please specify by full ID.",
            q
        ));
    }

    // 3. Exact account_id (workspace UUID) match
    let ws_matches: Vec<usize> = accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.account_id.trim().eq_ignore_ascii_case(q)
                || a.tokens
                    .account_id
                    .as_deref()
                    .map(|t| t.trim().eq_ignore_ascii_case(q))
                    == Some(true)
        })
        .map(|(i, _)| i)
        .collect();
    if ws_matches.len() == 1 {
        return Ok(ws_matches[0]);
    }

    // 4. Exact email match (unambiguous)
    let email_matches: Vec<usize> = accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| a.email.trim().eq_ignore_ascii_case(q))
        .map(|(i, _)| i)
        .collect();
    if email_matches.len() == 1 {
        return Ok(email_matches[0]);
    } else if email_matches.len() > 1 {
        let options: Vec<String> = email_matches
            .iter()
            .map(|&i| format!("{} ({})", accounts[i].display_name(), accounts[i].id))
            .collect();
        return Err(format!(
            "Multiple accounts found for email '{}': {}. Please specify by nickname or full ID.",
            q,
            options.join(", ")
        ));
    }

    // 5. Prefix match on ID, UUID, or nickname (if unambiguous and query length >= 3)
    if q.len() >= 3 {
        let prefix_matches: Vec<usize> = accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.id.to_lowercase().starts_with(&q.to_lowercase())
                    || a.account_id.to_lowercase().starts_with(&q.to_lowercase())
                    || a.name
                        .as_deref()
                        .map(|n| n.to_lowercase().starts_with(&q.to_lowercase()))
                        .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();
        if prefix_matches.len() == 1 {
            return Ok(prefix_matches[0]);
        }
    }

    Err(format!(
        "Account with ID, nickname, or email '{}' not found",
        query
    ))
}

#[cfg(test)]
#[path = "account_target_resolver.test.rs"]
mod tests;
