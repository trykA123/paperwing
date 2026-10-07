use std::collections::{BTreeMap, HashSet};

pub(super) struct ListedRemote {
    pub(super) name: String,
    pub(super) urls: Vec<String>,
    pub(super) complete: bool,
}

pub(super) fn parse_remote_verbose(text: &str) -> Vec<ListedRemote> {
    let mut remotes: BTreeMap<&str, (Vec<&str>, Vec<&str>)> = BTreeMap::new();
    for line in text.lines() {
        let Some((name, rest)) = line.split_once('\t') else {
            continue;
        };
        let entry = remotes.entry(name).or_default();
        if let Some(url) = rest.strip_suffix(" (fetch)") {
            entry.0.push(url);
        } else if let Some(url) = rest.strip_suffix(" (push)") {
            entry.1.push(url);
        }
    }
    remotes
        .into_iter()
        .map(|(name, (fetch, push))| {
            let complete =
                matches!(fetch.as_slice(), [url] if !url.is_empty() && push.as_slice() == [*url]);
            ListedRemote {
                name: name.into(),
                urls: fetch.iter().map(|url| url.to_string()).collect(),
                complete,
            }
        })
        .collect()
}

pub(super) fn pushurl_remotes(config: &str) -> HashSet<String> {
    config
        .lines()
        .filter_map(|line| line.split(' ').next())
        .filter_map(|key| {
            key.strip_prefix("remote")?
                .strip_prefix('.')?
                .strip_suffix(".pushurl")
        })
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
#[path = "remote_list_tests.rs"]
mod parity_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_url_remotes_are_complete() {
        let remotes = parse_remote_verbose(
            "origin\thttps://a/x.git (fetch)\norigin\thttps://a/x.git (push)\n",
        );
        assert_eq!(remotes.len(), 1);
        assert!(remotes[0].complete);
        assert_eq!(remotes[0].urls, ["https://a/x.git"]);
    }

    #[test]
    fn extra_or_different_push_urls_need_a_full_lookup() {
        let multi = parse_remote_verbose("origin\ta (fetch)\norigin\ta (push)\norigin\tb (push)\n");
        assert!(!multi[0].complete);
        let pushurl = parse_remote_verbose("origin\ta (fetch)\norigin\tp (push)\n");
        assert!(!pushurl[0].complete);
    }

    #[test]
    fn pushurl_names_are_read_from_config_keys() {
        let names =
            pushurl_remotes("remote.origin.pushurl https://x/y\nremote.my.fork.pushurl a b\n");
        assert!(names.contains("origin") && names.contains("my.fork"));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn remotes_come_back_sorted_by_name() {
        let remotes =
            parse_remote_verbose("b\tu (fetch)\nb\tu (push)\na\tv (fetch)\na\tv (push)\n");
        assert_eq!(
            remotes
                .iter()
                .map(|remote| remote.name.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
}
