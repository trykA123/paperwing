use super::{decode, hex, Context, Job, Problem};
use std::collections::{BTreeSet, HashMap};

pub(super) async fn validate(
    context: &Context,
    index: &HashMap<String, (String, String)>,
    job: &Job,
) -> Result<(), Problem> {
    let objects: BTreeSet<_> = index
        .values()
        .filter(|(mode, _)| mode != "160000")
        .map(|(_, oid)| oid.clone())
        .collect();
    let objects: Vec<_> = objects.into_iter().collect();
    for chunk in objects.chunks(1024) {
        let input = format!("{}\n", chunk.join("\n"));
        let result = job
            .run_input(
                &context.root,
                &["cat-file", "--batch-check"],
                &[0],
                Some(input.as_bytes()),
            )
            .await?;
        if result.code != Some(0) {
            return Err(Problem::new("gitError", "Index object inspection failed"));
        }
        for line in decode(&result.stdout)?.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 3 || fields[1] != "blob" || !hex(fields[0]) {
                return Err(Problem::new("gitError", "Invalid index object metadata"));
            }
            fields[2]
                .parse::<usize>()
                .map_err(|_| Problem::new("gitError", "Invalid blob size"))?;
        }
    }
    Ok(())
}
