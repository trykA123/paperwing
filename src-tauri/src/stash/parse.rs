use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StashEntry {
    pub index: usize,
    pub reference: String,
    pub oid: String,
    pub message: String,
    pub branch: Option<String>,
    pub created_at: i64,
}

fn split_subject(subject: &str) -> (Option<String>, String) {
    for prefix in ["WIP on ", "On "] {
        let Some(rest) = subject.strip_prefix(prefix) else {
            continue;
        };
        let Some((branch, message)) = rest.split_once(": ") else {
            continue;
        };
        let message = if prefix == "On " { message } else { subject };
        return (Some(branch.to_string()), message.to_string());
    }
    (None, subject.to_string())
}

pub fn parse_list(text: &str) -> Vec<StashEntry> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let fields: Vec<&str> = line.splitn(4, '\t').collect();
            let [reference, oid, time, subject] = fields[..] else {
                return None;
            };
            let (branch, message) = split_subject(subject);
            Some(StashEntry {
                index,
                reference: reference.into(),
                oid: oid.into(),
                message,
                branch,
                created_at: time.parse().ok()?,
            })
        })
        .collect()
}
