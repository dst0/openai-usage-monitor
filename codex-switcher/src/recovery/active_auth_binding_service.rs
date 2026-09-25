use crate::storage;

pub(super) struct ActiveAuthBindingService;

impl ActiveAuthBindingService {
    /// During a Desktop restart the auth file is committed before the CLI
    /// registry is updated. Resolve the actual auth identity uniquely instead
    /// of relying on an active-account pointer that can lag the transaction.
    pub(super) fn current() -> Option<String> {
        let accounts = storage::load_accounts().ok()?;
        let auth = storage::read_active_auth_json().ok()?;
        let tokens = auth.tokens.as_ref()?;
        let account_id = tokens.account_id.as_deref()?;
        let (email, _) = crate::oauth::extract_jwt_metadata_from_tokens(tokens);
        let mut matching = accounts.accounts.iter().filter(|account| {
            account.account_id == account_id
                && match email.as_deref() {
                    Some(email) => email.eq_ignore_ascii_case(&account.email),
                    None => {
                        !tokens.access_token.is_empty()
                            && tokens.access_token == account.tokens.access_token
                            && tokens.refresh_token == account.tokens.refresh_token
                    }
                }
        });
        let first = matching.next()?;
        if matching.next().is_some() {
            return None;
        }
        Some(first.id.clone())
    }
}
