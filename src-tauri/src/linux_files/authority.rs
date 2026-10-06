use super::{tickets::Ticket, Environment};
use crate::linux_guard::{
    mutation::{OperationAuthority, Snapshot},
    root::RootValue,
    Error, Root,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone)]
pub(super) struct Source {
    pub root: RootValue,
    pub path: String,
    pub expected: Snapshot,
}
pub(super) struct Authority<'a> {
    pub ticket: &'a Ticket,
    pub environment: &'a Environment,
    pub settings: &'a crate::settings::Settings,
    pub source: Option<&'a Source>,
    pub cancel: Option<&'a Arc<AtomicBool>>,
}
impl OperationAuthority for Authority<'_> {
    fn refresh(&mut self) -> Result<(), Error> {
        self.check()?;
        self.environment.revalidate(self.settings).map_err(|_| {
            Error::conflict("Registered settings changed or unavailable; reopen the comparison")
        })?;
        Root::reopen(&self.ticket.plan.root)?.probe_write()?;
        if let Some(source) = self.source {
            let root = Root::reopen(&source.root)?;
            root.parent(&source.path, false)?
                .validate(&source.expected)?;
        }
        self.check()
    }
    fn check(&self) -> Result<(), Error> {
        if self.ticket.check().is_err()
            || self
                .cancel
                .is_some_and(|cancel| cancel.load(Ordering::Acquire))
        {
            return Err(Error::conflict(
                "Comparison or file operation cancelled; destination retained",
            ));
        }
        Ok(())
    }
}
