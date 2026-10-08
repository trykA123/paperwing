use crate::search::{Match, RepoStatus};
use crate::search::{Plan, RepoTarget};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct Budget {
    claimed: AtomicUsize,
    capped: AtomicBool,
    overall: usize,
}

impl Budget {
    pub fn new(overall: usize) -> Self {
        Self {
            claimed: AtomicUsize::new(0),
            capped: AtomicBool::new(false),
            overall,
        }
    }

    pub fn is_capped(&self) -> bool {
        self.capped.load(Ordering::SeqCst)
    }

    pub(crate) fn claim(&self) -> bool {
        let fits = self.claimed.fetch_add(1, Ordering::SeqCst) < self.overall;
        if !fits {
            self.capped.store(true, Ordering::SeqCst);
        }
        fits
    }
}

pub struct RepoResult {
    pub status: RepoStatus,
    pub matches: Vec<Match>,
}

pub struct RepoSearch<'a> {
    pub target: &'a RepoTarget,
    pub plan: &'a Plan,
    pub budget: Arc<Budget>,
    pub cancel: Arc<AtomicBool>,
}

pub trait SearchEngine: Send + Sync {
    fn search<'a>(
        &'a self,
        search: RepoSearch<'a>,
    ) -> Pin<Box<dyn Future<Output = RepoResult> + Send + 'a>>;
}

pub struct GitGrep;

impl SearchEngine for GitGrep {
    fn search<'a>(
        &'a self,
        search: RepoSearch<'a>,
    ) -> Pin<Box<dyn Future<Output = RepoResult> + Send + 'a>> {
        Box::pin(crate::search_grep::search_repo(
            search.target,
            search.plan,
            search.budget,
            search.cancel,
        ))
    }
}

#[derive(Clone, Copy, Default)]
pub enum Engine {
    #[default]
    BuiltIn,
    GitGrep,
}

impl SearchEngine for Engine {
    fn search<'a>(
        &'a self,
        search: RepoSearch<'a>,
    ) -> Pin<Box<dyn Future<Output = RepoResult> + Send + 'a>> {
        let reason = self.fallback_reason(search.target, search.plan);
        let engine = if reason.is_some() {
            Self::GitGrep
        } else {
            *self
        };
        Box::pin(async move {
            let mut result = match engine {
                Self::GitGrep => GitGrep.search(search).await,
                Self::BuiltIn => crate::search_builtin::BuiltIn.search(search).await,
            };
            if let Some(reason) = reason {
                result.status.engine_note = Some(reason.into());
            }
            result
        })
    }
}

impl Engine {
    fn fallback_reason(&self, target: &RepoTarget, plan: &Plan) -> Option<&'static str> {
        if matches!(self, Self::GitGrep) {
            return None;
        }
        if plan.flags.contains(&"-P") {
            return Some("Using Git grep: Perl expressions require Git grep.");
        }
        if target.git_ref.is_some() {
            return Some("Using Git grep: committed refs require Git grep.");
        }
        if crate::search_pattern::needs_git(plan) {
            return Some("Using Git grep: this basic-regex construct requires Git grep.");
        }
        None
    }
}

impl From<crate::settings::SearchEngine> for Engine {
    fn from(engine: crate::settings::SearchEngine) -> Self {
        match engine {
            crate::settings::SearchEngine::BuiltIn => Self::BuiltIn,
            crate::settings::SearchEngine::GitGrep => Self::GitGrep,
        }
    }
}
