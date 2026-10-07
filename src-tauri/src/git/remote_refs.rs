use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use super::{buffered, valid_url};

pub(super) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c1), Some(c2)) if c1.is_ascii_digit() && c2.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(&c) = it.peek() {
                        if !c.is_ascii_digit() {
                            break;
                        }
                        s.push(c);
                        it.next();
                    }
                    s.trim_start_matches('0').to_string()
                };
                let (n1, n2) = (take(&mut x), take(&mut y));
                let o = n1.len().cmp(&n2.len()).then_with(|| n1.cmp(&n2));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c1), Some(c2)) => {
                let o = c1.to_ascii_lowercase().cmp(&c2.to_ascii_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

type RefList = Vec<(String, String)>;

fn parse_refs(text: &str) -> (RefList, RefList) {
    let mut branches: Vec<(String, String)> = Vec::new();
    let mut tags: Vec<(String, String)> = Vec::new();
    let mut tag_index: HashMap<String, usize> = HashMap::new();
    for line in text.lines() {
        let Some((sha, r)) = line.split_once('\t') else { continue };
        if let Some(b) = r.strip_prefix("refs/heads/") {
            branches.push((b.to_string(), sha.to_string()));
        } else if let Some(t) = r.strip_prefix("refs/tags/") {
            // Annotated tags appear twice; the peeled `^{}` line carries the commit SHA.
            let (name, peeled) = t.strip_suffix("^{}").map_or((t, false), |n| (n, true));
            match tag_index.get(name) {
                Some(&index) if peeled => tags[index].1 = sha.to_string(),
                Some(_) => {}
                None => {
                    tag_index.insert(name.to_string(), tags.len());
                    tags.push((name.to_string(), sha.to_string()));
                }
            }
        }
    }
    (branches, tags)
}

async fn remote_refs(url: &str) -> Result<RefsResult, String> {
    valid_url(url)?;
    let out = buffered(&["ls-remote", "--heads", "--tags", "--", url], "Remote references", &[0]).await?;
    if out.code != Some(0) {
        return Err(out.last_error());
    }
    let (mut branches, mut tags) = parse_refs(&String::from_utf8_lossy(&out.stdout));
    branches.sort_by(|a, b| natural_cmp(&a.0, &b.0));
    tags.sort_by(|a, b| natural_cmp(&b.0, &a.0));
    let branch_labels = branches.iter().map(|(name, _)| out.safe(name)).collect();
    let tag_labels = tags.iter().map(|(name, _)| out.safe(name)).collect();
    let (branches, branch_shas) = branches.into_iter().unzip();
    let (tags, tag_shas) = tags.into_iter().unzip();
    Ok(RefsResult { url: url.into(), branches, tags, branch_labels, tag_labels, branch_shas, tag_shas, error: None })
}

#[cfg(test)]
pub(super) async fn ls_remote(url: &str) -> Result<(Vec<(String, String)>, Vec<(String, String)>), String> {
    let refs = remote_refs(url).await?;
    Ok((refs.branches.into_iter().zip(refs.branch_shas).collect(), refs.tags.into_iter().zip(refs.tag_shas).collect()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefsResult {
    url: String,
    branches: Vec<String>,
    tags: Vec<String>,
    branch_labels: Vec<String>,
    tag_labels: Vec<String>,
    branch_shas: Vec<String>,
    tag_shas: Vec<String>,
    error: Option<String>,
}

pub(crate) fn failed_refs(url: String, error: String) -> RefsResult {
    RefsResult { url, branches: vec![], tags: vec![], branch_labels: vec![], tag_labels: vec![], branch_shas: vec![], tag_shas: vec![], error: Some(error) }
}

pub(crate) async fn get_refs_many(urls: Vec<String>) -> Vec<RefsResult> {
    let names = urls.clone();
    crate::ordered::map_bounded(
        urls,
        8,
        |url| async move {
            match remote_refs(&url).await {
                Ok(refs) => refs,
                Err(e) => failed_refs(url, e),
            }
        },
        move |index| failed_refs(names[index].clone(), "Remote refs check did not finish".into()),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peeled_tags_replace_the_tag_object_sha_in_any_order() {
        let text = "a1\trefs/tags/v1\nc1\trefs/tags/v1^{}\nc2\trefs/tags/v2^{}\na2\trefs/tags/v2\nb1\trefs/heads/main\n";
        let (branches, tags) = parse_refs(text);
        assert_eq!(branches, [("main".to_string(), "b1".to_string())]);
        assert_eq!(tags, [("v1".to_string(), "c1".to_string()), ("v2".to_string(), "c2".to_string())]);
    }

    #[test]
    fn ten_thousand_annotated_tags_keep_their_peeled_shas() {
        let mut text = String::new();
        for index in 0..10_000 { text.push_str(&format!("t{index}\trefs/tags/v{index}\nc{index}\trefs/tags/v{index}^{{}}\n")); }
        let (_, tags) = parse_refs(&text);
        assert_eq!(tags.len(), 10_000);
        assert!(tags.iter().enumerate().all(|(index, (name, sha))| *name == format!("v{index}") && *sha == format!("c{index}")));
    }
}
