use super::{
    metadata, sync_directory, Error, MutationError, OperationAuthority, Parent, Published,
    Snapshot, Staged,
};

pub(crate) struct AuthorizedPublication<'a, F, G> {
    pub expected: &'a Snapshot,
    pub security: &'a metadata::Security,
    pub caller: &'a mut dyn OperationAuthority,
    pub binding: F,
    pub after_mutation: G,
}
impl Parent {
    pub(crate) fn publish_authorized<F, G>(
        &self,
        stage: Staged,
        authority: AuthorizedPublication<'_, F, G>,
    ) -> Result<Published, MutationError>
    where
        F: FnOnce() -> Result<(), Error>,
        G: FnOnce() -> Result<(), Error>,
    {
        let AuthorizedPublication {
            expected,
            security,
            caller,
            binding,
            after_mutation,
        } = authority;
        caller.refresh()?;
        self.publish_inner(
            stage,
            expected,
            Some(security),
            || {
                binding()?;
                caller.check()
            },
            after_mutation,
            sync_directory,
        )
    }
}
