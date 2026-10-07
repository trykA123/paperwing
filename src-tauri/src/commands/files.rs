#[cfg(windows)]
use crate::files;
#[cfg(target_os = "linux")]
use crate::linux_files;
#[cfg(not(any(windows, target_os = "linux")))]
use crate::unsupported_files;

domain! {
            #[cfg(windows)] files::file_edit_open,
            #[cfg(target_os = "linux")] linux_files::file_edit_open,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::file_edit_open,
            #[cfg(windows)] files::file_edit_close,
            #[cfg(target_os = "linux")] linux_files::file_edit_close,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::file_edit_close,
            #[cfg(windows)] files::file_save,
            #[cfg(target_os = "linux")] linux_files::file_save,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::file_save,
            #[cfg(windows)] files::copy_preview,
            #[cfg(target_os = "linux")] linux_files::copy_preview,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::copy_preview,
            #[cfg(windows)] files::copy_apply,
            #[cfg(target_os = "linux")] linux_files::copy_apply,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::copy_apply,
            #[cfg(windows)] files::copy_cancel,
            #[cfg(target_os = "linux")] linux_files::copy_cancel,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::copy_cancel,
            #[cfg(windows)] files::recovery_list,
            #[cfg(target_os = "linux")] linux_files::recovery_list,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::recovery_list,
            #[cfg(windows)] files::recovery_undo,
            #[cfg(target_os = "linux")] linux_files::recovery_undo,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::recovery_undo,
            #[cfg(windows)] files::recovery_cleanup,
            #[cfg(target_os = "linux")] linux_files::recovery_cleanup,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::recovery_cleanup,
            #[cfg(windows)] files::recovery_resolve,
            #[cfg(target_os = "linux")] linux_files::recovery_resolve,
            #[cfg(not(any(windows, target_os = "linux")))] unsupported_files::recovery_resolve,
}
