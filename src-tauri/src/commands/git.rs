use crate::git;
use crate::history;

domain! {
            git::get_refs_many,
            git::activity_snapshot,
            git::clear_activity,
            git::cancel_activity,
            git::repository_tree,
            history::repository_history,
}
