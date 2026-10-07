use crate::clone;
use crate::launch;
use crate::local;
use crate::platform;
use crate::{__cmd__open_in_vscode, __tauri_command_name_open_in_vscode, open_in_vscode};

domain! {
            clone::start_clone,
            local::local_status,
            platform::platform_info,
            platform::probe_root,
            platform::path_identities,
            open_in_vscode,
            launch::launch_request,
}
