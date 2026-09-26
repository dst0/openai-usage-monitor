use super::dispatch_mark_error::DispatchMarkError;

type IdentityVerifier<'a> = Box<dyn FnMut() -> Result<(), DispatchMarkError> + 'a>;
type DeferredBindingResolver<'a> = Box<dyn FnMut() -> Option<String> + 'a>;

/// Identity evidence that must still hold when a recovery checkpoint is
/// consumed for an owner-routed IPC request.
///
/// Production binds these to `RecoveryDispatchIdentityGuard`, whose checks and
/// deferred binding read live auth, the ChatGPT process table, and the
/// installed window helper. Tests inject fakes instead.
pub(super) struct DispatchIdentityChecks<'a> {
    verify: IdentityVerifier<'a>,
    deferred_binding: DeferredBindingResolver<'a>,
}

impl<'a> DispatchIdentityChecks<'a> {
    pub(super) fn new(
        verify: impl FnMut() -> Result<(), DispatchMarkError> + 'a,
        deferred_binding: impl FnMut() -> Option<String> + 'a,
    ) -> Self {
        Self {
            verify: Box::new(verify),
            deferred_binding: Box::new(deferred_binding),
        }
    }

    /// Rechecks the operation's starting account and exact Desktop process.
    pub(super) fn verify(&mut self) -> Result<(), DispatchMarkError> {
        (self.verify)()
    }

    /// The Desktop account that may consume a deferred (awaiting-owner)
    /// checkpoint in an unattended mode, or `None` when it cannot be verified.
    pub(super) fn deferred_binding(&mut self) -> Option<String> {
        (self.deferred_binding)()
    }
}
