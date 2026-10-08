use tauri::ipc::Invoke;

struct Domain {
    names: &'static [&'static str],
    invoke: Box<dyn Fn(Invoke) -> bool + Send + Sync>,
}

macro_rules! domain {
    ($($(#[$attribute:meta])* $first:ident $(::$module:ident)*,)+) => {
        pub(super) fn handler() -> super::Domain {
            super::Domain {
                names: &[$($(#[$attribute])* domain!(@name $first $(::$module)*)),+],
                invoke: Box::new(tauri::generate_handler![$($(#[$attribute])* $first $(::$module)*),+]),
            }
        }
    };
    (@name $head:ident ::$($tail:ident)::+) => { domain!(@name $($tail)::+) };
    (@name $name:ident) => { stringify!($name) };
}

mod application;
mod changes;
mod ci;
mod compare;
mod files;
mod git;
mod instrumentation;
mod jobs;
mod repositories;
mod settings;
mod watch;

pub(crate) fn compose() -> impl Fn(Invoke) -> bool + Send + Sync + 'static {
    let domains = [
        application::handler(),
        changes::handler(),
        ci::handler(),
        compare::handler(),
        files::handler(),
        git::handler(),
        instrumentation::handler(),
        jobs::handler(),
        repositories::handler(),
        settings::handler(),
        watch::handler(),
    ];
    move |invoke| {
        let Some(domain) = domains
            .iter()
            .find(|domain| domain.names.contains(&invoke.message.command()))
        else {
            return false;
        };
        (domain.invoke)(invoke)
    }
}
