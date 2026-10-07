use std::ffi::OsString;

fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

pub(super) fn effective_ctype(
    lc_all: Option<OsString>,
    lc_ctype: Option<OsString>,
) -> Option<OsString> {
    non_empty(lc_all).or_else(|| non_empty(lc_ctype))
}

#[cfg(test)]
pub(crate) static CTYPE_OVERRIDE: std::sync::Mutex<Option<OsString>> = std::sync::Mutex::new(None);

pub(super) fn apply(command: &mut tokio::process::Command) {
    let ctype = effective_ctype(std::env::var_os("LC_ALL"), std::env::var_os("LC_CTYPE"));
    #[cfg(test)]
    let ctype = CTYPE_OVERRIDE.lock().unwrap().clone().or(ctype);
    command.env_remove("LC_ALL");
    if let Some(ctype) = ctype {
        command.env("LC_CTYPE", ctype);
    }
    command.env("LC_MESSAGES", "C").env("LANGUAGE", "");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lc_all_wins_over_lc_ctype_and_empty_values_are_ignored() {
        let value = |text: &str| Some(OsString::from(text));
        assert_eq!(
            effective_ctype(value("C.UTF-8"), value("de_DE.UTF-8")),
            value("C.UTF-8")
        );
        assert_eq!(
            effective_ctype(value(""), value("de_DE.UTF-8")),
            value("de_DE.UTF-8")
        );
        assert_eq!(effective_ctype(None, None), None);
    }
}
