use super::*;
use classification::{folder, Classifier};
use listing::Listed;

struct Ordered<'a> {
    listed: &'a Listed,
    bases: Vec<FileRow>,
    final_rows: Vec<Option<FileRow>>,
    remaining: Vec<usize>,
}

impl<'a> Ordered<'a> {
    fn new(listed: &'a Listed, classifier: &Classifier<'_>) -> Self {
        Self {
            listed,
            bases: listed.rows.iter().map(|row| classifier.row(row)).collect(),
            final_rows: vec![None; listed.rows.len()],
            remaining: listed.children.iter().map(Vec::len).collect(),
        }
    }

    fn commit(&mut self, mut index: usize, mut row: FileRow) -> Vec<FileRow> {
        let mut updates = Vec::new();
        loop {
            updates.push(row.clone());
            self.final_rows[index] = Some(row);
            let Some(parent) = self.listed.parents[index] else {
                break;
            };
            self.remaining[parent] -= 1;
            if self.remaining[parent] != 0 || self.final_rows[parent].is_some() {
                break;
            }
            let mut parent_row = self.bases[parent].clone();
            if !folder(&parent_row) {
                break;
            }
            self.roll_up(parent, &mut parent_row);
            index = parent;
            row = parent_row;
        }
        updates
    }

    fn roll_up(&self, index: usize, row: &mut FileRow) {
        if row.raw_status != Status::Same {
            return;
        }
        let children: Vec<_> = self.listed.children[index]
            .iter()
            .filter_map(|&child| self.final_rows[child].as_ref())
            .collect();
        let roll = |display| {
            let statuses: Vec<_> = children
                .iter()
                .map(|row| {
                    if display {
                        &row.display_status
                    } else {
                        &row.raw_status
                    }
                })
                .collect();
            if statuses.contains(&&Status::Unavailable) {
                Status::Unavailable
            } else if statuses.iter().all(|status| **status == Status::Same) {
                Status::Same
            } else {
                Status::Different
            }
        };
        row.raw_status = roll(false);
        row.display_status = roll(true);
    }
}

pub(super) async fn commit(
    service: &Service,
    identity: (&str, u64),
    listed: &Listed,
    classifier: Classifier<'_>,
) -> Result<Vec<FileRow>, Problem> {
    let mut ordered = Ordered::new(listed, &classifier);
    for index in 0..ordered.bases.len() {
        classifier.job.check()?;
        if ordered.final_rows[index].is_some() {
            continue;
        }
        let mut row = ordered.bases[index].clone();
        if classifier.fixed(&mut row) || (folder(&row) && ordered.remaining[index] == 0) {
            let updates = ordered.commit(index, row);
            service.publish_final(identity, updates).await?;
        }
        if index % progressive::PAGE_ROWS == 0 {
            tokio::task::yield_now().await;
        }
    }
    #[cfg(test)]
    service.defer_enrichment(classifier.job, true).await?;
    let mut used = 0;
    for index in 0..ordered.bases.len() {
        if ordered.final_rows[index].is_some() || folder(&ordered.bases[index]) {
            continue;
        }
        let row = classifier
            .commit(ordered.bases[index].clone(), &mut used)
            .await?;
        let updates = ordered.commit(index, row);
        service.publish_final(identity, updates).await?;
        tokio::task::yield_now().await;
    }
    ordered
        .final_rows
        .into_iter()
        .map(|row| row.ok_or_else(|| Problem::new("internal", "Comparison row did not commit")))
        .collect()
}
