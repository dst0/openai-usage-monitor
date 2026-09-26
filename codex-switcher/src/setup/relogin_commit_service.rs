use crate::{
    models::{AccountConfig, AccountsFile, AuthJson},
    oauth::extract_jwt_metadata,
};

/// Commits a staged browser login only after the shared Desktop credential
/// store is proven idle and still belongs to the account being refreshed.
pub(super) struct ReloginCommitService<'a> {
    original: &'a AccountConfig,
    staged: &'a AccountsFile,
    updated_id: &'a str,
}

impl<'a> ReloginCommitService<'a> {
    pub(super) fn new(
        original: &'a AccountConfig,
        staged: &'a AccountsFile,
        updated_id: &'a str,
    ) -> Self {
        Self {
            original,
            staged,
            updated_id,
        }
    }

    pub(super) fn commit_with(
        &self,
        mut shared_auth_active: impl FnMut() -> Result<bool, String>,
        mut read_auth: impl FnMut() -> Result<AuthJson, String>,
        mut compare_and_write_auth: impl FnMut(&AuthJson, &AuthJson) -> Result<(), String>,
        mut commit_registry: impl FnMut(&AccountsFile, Option<&AuthJson>) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.staged.active_account_id.as_deref() != Some(self.updated_id) {
            return commit_registry(self.staged, None);
        }
        if shared_auth_active()? {
            return Err("ChatGPT is using shared credentials; close it before re-login of the active account".into());
        }
        let previous = read_auth()?;
        self.validate_live_auth(&previous)?;
        if shared_auth_active()? {
            return Err("Shared credentials became active before re-login commit".into());
        }
        let current = read_auth()?;
        if !same_auth(&previous, &current) {
            return Err("Shared credentials changed before re-login commit".into());
        }
        let mut final_tokens = self
            .staged
            .accounts
            .iter()
            .find(|account| account.id == self.updated_id)
            .ok_or("Re-login target disappeared before commit")?
            .tokens
            .clone();
        if let Some(previous_tokens) = previous.tokens.as_ref() {
            for (key, value) in &previous_tokens.extra {
                final_tokens
                    .extra
                    .entry(key.clone())
                    .or_insert_with(|| value.clone());
            }
        }
        let mut committed_registry = self.staged.clone();
        committed_registry
            .accounts
            .iter_mut()
            .find(|account| account.id == self.updated_id)
            .ok_or("Re-login target disappeared before commit")?
            .tokens = final_tokens.clone();
        let mut replacement = previous;
        replacement.tokens = Some(final_tokens.clone());
        replacement.last_refresh = Some(chrono::Utc::now().to_rfc3339());
        compare_and_write_auth(&current, &replacement)?;
        let committed = read_auth()
            .map_err(|_| "Active credentials were written but could not be verified".to_string())?;
        if !same_auth(&committed, &replacement) {
            return Err(
                "Active credentials changed after re-login commit; registry was not overwritten"
                    .into(),
            );
        }
        commit_registry(&committed_registry, Some(&replacement)).map_err(|error| {
            format!("Active credentials were committed, but accounts registry save failed: {error}")
        })
    }

    fn validate_live_auth(&self, auth: &AuthJson) -> Result<(), String> {
        if auth.auth_mode.as_deref() != Some("chatgpt") {
            return Err("Active credentials are not a ChatGPT login".into());
        }
        let live = auth
            .tokens
            .as_ref()
            .ok_or("Active ChatGPT tokens are missing")?;
        let expected_workspace = self
            .original
            .tokens
            .account_id
            .as_deref()
            .filter(|id| !id.is_empty() && *id != "default")
            .unwrap_or(self.original.account_id.as_str());
        if expected_workspace.is_empty()
            || expected_workspace == "default"
            || live.account_id.as_deref() != Some(expected_workspace)
        {
            return Err(
                "Active ChatGPT account identity does not match the re-login target".into(),
            );
        }
        let mut email_matches = false;
        for token in [live.id_token.as_deref(), Some(live.access_token.as_str())] {
            if let Some(email) = extract_jwt_metadata(token).0 {
                if !email.contains('@')
                    || !email
                        .trim()
                        .eq_ignore_ascii_case(self.original.email.trim())
                {
                    return Err(
                        "Active ChatGPT user identity does not match the re-login target".into(),
                    );
                }
                email_matches = true;
            }
        }
        let token_continuity = live.access_token == self.original.tokens.access_token
            || live.refresh_token.as_deref().is_some_and(|token| {
                !token.is_empty() && self.original.tokens.refresh_token.as_deref() == Some(token)
            });
        if !email_matches && !token_continuity {
            return Err("Active ChatGPT user identity does not match the re-login target".into());
        }
        Ok(())
    }
}

fn same_auth(left: &AuthJson, right: &AuthJson) -> bool {
    left == right
}
