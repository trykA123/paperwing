use crate::credentials;
use crate::settings;

domain! {
            settings::commands::load_settings,
            settings::commands::save_settings,
            settings::set_token,
            settings::has_token,
            settings::delete_token,
            credentials::credential_status,
            credentials::source_revision,
}
