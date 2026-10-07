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
        let engine = if search.target.git_ref.is_some()
            || search.plan.flags.contains(&"-P")
            || crate::search_pattern::needs_git(search.plan)
        {
            Self::GitGrep
        } else {
            *self
        };
        match engine {
            Self::GitGrep => GitGrep.search(search),
            Self::BuiltIn => crate::search_builtin::BuiltIn.search(search),
        }
    }
}
