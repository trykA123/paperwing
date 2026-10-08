use super::*;

pub(super) struct Listed {
    pub left: Resolved,
    pub right: Resolved,
    pub rows: Vec<PendingRow>,
    pub indices: HashMap<String, usize>,
    pub children: Vec<Vec<usize>>,
    pub parents: Vec<Option<usize>>,
}

impl Listed {
    pub fn new(left: Resolved, right: Resolved, paths: BTreeSet<String>) -> Self {
        let mut rows: Vec<_> = paths
            .into_iter()
            .map(|path| PendingRow {
                id: format!("file-{}", NEXT.fetch_add(1, Ordering::Relaxed)),
                left: left.files.get(&path).map(SideInfo::from),
                right: right.files.get(&path).map(SideInfo::from),
                hint: hint(&left, &right, &path),
                path,
            })
            .collect();
        let (children, parents) = topology(&rows);
        aggregate_folders(&mut rows, &children);
        let indices = rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id.clone(), index))
            .collect();
        Self {
            left,
            right,
            rows,
            indices,
            children,
            parents,
        }
    }
}

fn topology(rows: &[PendingRow]) -> (Vec<Vec<usize>>, Vec<Option<usize>>) {
    let paths: HashMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.path.as_str(), index))
        .collect();
    let mut children = vec![Vec::new(); rows.len()];
    let mut parents = vec![None; rows.len()];
    for (index, row) in rows.iter().enumerate() {
        if let Some((parent, _)) = row.path.rsplit_once('/') {
            if let Some(&parent) = paths.get(parent) {
                children[parent].push(index);
                parents[index] = Some(parent);
            }
        }
    }
    (children, parents)
}

fn aggregate_folders(rows: &mut [PendingRow], children: &[Vec<usize>]) {
    for index in (0..rows.len()).rev() {
        for is_left in [true, false] {
            let sides: Vec<_> = children[index]
                .iter()
                .filter_map(|&child| {
                    if is_left {
                        rows[child].left.as_ref()
                    } else {
                        rows[child].right.as_ref()
                    }
                })
                .collect();
            let size = sides.iter().try_fold(0u64, |sum, side| {
                side.size.and_then(|size| sum.checked_add(size))
            });
            let modified_ms = sides.iter().filter_map(|side| side.modified_ms).max();
            let side = if is_left {
                &mut rows[index].left
            } else {
                &mut rows[index].right
            };
            if let Some(side) = side.as_mut().filter(|side| side.kind == Kind::Directory) {
                side.size = size;
                side.modified_ms = modified_ms;
            }
        }
    }
}

fn hint(left: &Resolved, right: &Resolved, path: &str) -> Hint {
    let entries = [left.files.get(path), right.files.get(path)];
    if entries
        .into_iter()
        .flatten()
        .any(|entry| entry.source == "untrackedRepository")
    {
        return Hint::Opaque;
    }
    if entries
        .into_iter()
        .flatten()
        .any(|entry| entry.reason.is_some())
    {
        return Hint::Unavailable;
    }
    match entries {
        [Some(left), Some(right)] if left.kind != right.kind => Hint::TypeConflict,
        [Some(left), Some(_)] if left.kind == Kind::Directory => Hint::Folder,
        [Some(_), None] => Hint::LeftOnly,
        [None, Some(_)] => Hint::RightOnly,
        _ if classification::identical(left, right, path) => Hint::SameId,
        _ => Hint::ChangedId,
    }
}
