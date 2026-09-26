pub(super) struct RecoveryErrorSanitizer;

impl RecoveryErrorSanitizer {
    pub(super) fn sanitize(error: &str) -> String {
        error
            .chars()
            .filter(|character| !character.is_control())
            .take(160)
            .collect()
    }
}
