use crate::github::actions::commands;

domain! {
    commands::ci_runs,
    commands::ci_jobs,
    commands::ci_job_log,
    commands::ci_artifacts,
    commands::ci_download_artifact,
    commands::ci_download_run_logs,
    commands::ci_rerun,
    commands::ci_cancel,
    commands::ci_dispatch_inputs,
    commands::ci_dispatch,
}
