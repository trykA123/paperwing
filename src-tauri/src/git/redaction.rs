use super::configured_sources;
use std::sync::Mutex;

const QUIET_TEXT: &str = "Git output omitted because the credential store is inaccessible.";

pub(super) type SecretKey = Vec<(String, u64)>;

static SECRET_CACHE: Mutex<Option<(SecretKey, Vec<String>)>> = Mutex::new(None);

pub(super) fn secret_key(ids: &[String]) -> SecretKey {
    ids.iter().map(|id| (id.clone(), crate::credentials::revision(id))).collect()
}

pub(super) fn cached_secrets(key: &SecretKey) -> Option<Vec<String>> {
    SECRET_CACHE.lock().unwrap().as_ref().filter(|(cached, _)| cached == key).map(|(_, secrets)| secrets.clone())
}

pub(super) fn remember_secrets(key: SecretKey, secrets: &[String]) {
    *SECRET_CACHE.lock().unwrap() = Some((key, secrets.to_vec()));
}

fn last_good_secrets() -> Option<Vec<String>> {
    if configured_sources().is_empty() { return Some(Vec::new()); }
    SECRET_CACHE.lock().unwrap().as_ref().map(|(_, secrets)| secrets.clone())
}

#[derive(Clone)]
pub(super) enum Redaction {
    Ready(Vec<String>),
    Quiet,
}

impl Redaction {
    pub(super) fn safe(&self, text: &str) -> String {
        match self {
            Self::Ready(secrets) => redact(text, secrets),
            Self::Quiet => QUIET_TEXT.into(),
        }
    }

    pub(super) fn secrets(&self) -> &[String] {
        match self { Self::Ready(secrets) => secrets, Self::Quiet => &[] }
    }

    pub(super) fn quiet(&self) -> bool { matches!(self, Self::Quiet) }
}

pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut clean = text.to_string();
    let mut ordered = secrets.to_vec();
    ordered.sort_by_key(|value| std::cmp::Reverse(value.len()));
    for secret in ordered {
        clean = clean.replace(&secret, "[redacted]");
    }
    let mut authorities = String::new();
    let mut remaining = clean.as_str();
    while let Some(scheme) = remaining.find("://") {
        let start = scheme + 3;
        authorities.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let mut end = remaining.len();
        let mut cursor = 0;
        while let Some(boundary) = remaining[cursor..].find(['/', '?', '#']) {
            let index = cursor + boundary;
            if index > 0 && remaining.as_bytes()[index - 1] == b':' && remaining[index..].starts_with("//") {
                let prefix = &remaining[..index - 1];
                if let Some((_, next_scheme)) = prefix.rsplit_once(char::is_whitespace) {
                    if reqwest::Url::parse(&format!("{next_scheme}://invalid.test")).is_ok_and(|url| {
                        url.scheme().eq_ignore_ascii_case(next_scheme) && url.host_str() == Some("invalid.test")
                    }) {
                        end = prefix.len() - next_scheme.len();
                        break;
                    }
                }
                cursor = index + 2;
            } else {
                end = index;
                break;
            }
        }
        let authority = &remaining[..end];
        let value = authority.trim_end_matches(char::is_whitespace);
        let valid_authority = !value.chars().any(char::is_whitespace) && !value.contains("://")
            && reqwest::Url::parse(&format!("https://{value}")).is_ok_and(|url| {
                url.has_host() && url.username().is_empty() && url.password().is_none() && url.path() == "/"
                    && url.query().is_none() && url.fragment().is_none()
            });
        let redact_end = value.rfind('@').or_else(|| {
            (!value.is_empty() && !valid_authority).then_some(value.len())
        });
        if let Some(redact_end) = redact_end {
            authorities.push_str("[redacted]");
            authorities.extend(value[..redact_end].chars().filter(|character| character.is_whitespace()));
            authorities.push_str(&authority[redact_end..]);
        } else {
            authorities.push_str(authority);
        }
        remaining = &remaining[end..];
        let tail_end = remaining.find(char::is_whitespace).unwrap_or(remaining.len())
            .min(remaining.find("://").unwrap_or(remaining.len()));
        let tail = &remaining[..tail_end];
        if let Some(query) = tail.find(['?', '#']) {
            authorities.push_str(&tail[..query]);
            authorities.push_str("?[redacted]");
        } else {
            authorities.push_str(tail);
        }
        remaining = &remaining[tail_end..];
    }
    authorities.push_str(remaining);
    let mut result = String::new();
    for line in authorities.split_inclusive(['\r', '\n']) {
        let lower = line.to_ascii_lowercase();
        if let Some(index) = ["authorization:", "proxy-authorization:", "cookie:", "set-cookie:"]
            .iter().filter_map(|header| lower.find(header)).min() {
            result.push_str(&line[..index]);
            result.push_str("[redacted header]\n");
            continue;
        }
        result.push_str(line);
    }
    result
}

pub fn safe(text: &str) -> String {
    match last_good_secrets() {
        Some(secrets) => redact(text, &secrets),
        None => QUIET_TEXT.into(),
    }
}

/// Picks the most useful line out of git's stderr, with a hint for common SSH failures.
pub fn last_error(stderr: &str) -> String {
    let clean = redact(stderr, &[]);
    let stderr = clean.as_str();
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let has = |needle: &str| lines.iter().find(|l| l.contains(needle)).copied();
    if let Some(l) = has("Permission denied (publickey") {
        let host = l.split(": Permission").next().unwrap_or("the host");
        return format!("SSH key rejected by {host}. Check that your key is added under Settings → SSH keys on the server (test with: ssh -T {host})");
    }
    if has("Host key verification failed").is_some() {
        return "Unknown SSH host key. Connect once from a terminal (ssh -T git@host) and accept the fingerprint".into();
    }
    if let Some(l) = has("Could not resolve hostname").or_else(|| has("Connection timed out")).or_else(|| has("Connection refused")) {
        return format!("Network problem: {}", l.trim_start_matches("ssh: "));
    }
    if has("Repository not found").is_some() || has("does not appear to be a git repository").is_some() {
        return "Repository not found, or you have no access to it".into();
    }
    let line = lines
        .iter()
        .find(|l| l.starts_with("fatal:") || l.starts_with("error:"))
        .or(lines.last())
        .copied()
        .unwrap_or("git failed");
    line.trim_start_matches("fatal: ").trim_start_matches("error: ").to_string()
}
