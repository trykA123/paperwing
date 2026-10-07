use crate::branch_cleanup;
use crate::commit;
use crate::stash;
use crate::tags;
use crate::trash;

domain! {
            commit::repo_changes,
            commit::change_content,
            commit::stage_paths,
            commit::unstage_paths,
            commit::commit_staged,
            commit::create_branch,
            commit::push_branch,
            commit::delete_branch,
            tags::list_tags,
            tags::create_tag,
            tags::push_tag,
            tags::delete_tag,
            stash::stash_list,
            stash::stash_push,
            stash::stash_apply,
            stash::stash_pop,
            stash::stash_drop,
            stash::stash_show,
            stash::switch_with_stash,
            branch_cleanup::merged_branches,
            branch_cleanup::delete_merged_branches,
            trash::trash_set_folders,
}
