use super::*;

const FETCH_ARGS: &[&str] = &[
    "-c",
    "gc.auto=0",
    "-c",
    "maintenance.auto=false",
    "-c",
    "protocol.ext.allow=never",
    "-c",
    "fetch.prune=false",
    "-c",
    "fetch.pruneTags=false",
    "-c",
    "remote.origin.prune=false",
    "-c",
    "remote.origin.pruneTags=false",
    "fetch",
    "--no-prune",
    "--no-write-fetch-head",
    "--no-auto-maintenance",
    "--no-recurse-submodules",
    "--",
    "origin",
];

pub(super) struct References<'a> {
    pub contexts: [&'a Context; 2],
    pub roots: [&'a PathBuf; 2],
    pub states: Vec<Arc<Mutex<Fetch>>>,
    pub epochs: Vec<u64>,
    pub job: &'a Job,
}

pub(super) async fn resolve_refs(input: References<'_>) -> Result<[String; 2], Problem> {
    let mut commits = Vec::new();
    for (index, context) in input.contexts.iter().enumerate() {
        commits.push(
            resolve(context, input.job)
                .await
                .map_err(|problem| problem.side(side(index)))?,
        );
    }
    let mut attempted = BTreeSet::new();
    for (index, commit) in commits.iter_mut().enumerate() {
        if commit.is_some() {
            continue;
        }
        let mut state = input.job.lock(&input.states[index]).await?;
        if state.epoch == input.epochs[index]
            && attempted.insert(input.roots[index].clone())
            && resolve(input.contexts[index], input.job).await?.is_none()
        {
            let result = fetch_origin(input.contexts[index], input.job).await;
            state.epoch += 1;
            state.problem = result?;
        }
        *commit = resolve(input.contexts[index], input.job)
            .await
            .map_err(|problem| problem.side(side(index)))?;
        if commit.is_none() {
            if let Some(problem) = &state.problem {
                return Err(problem.clone().side(side(index)));
            }
            return Err(Problem::new(
                if index == 0 {
                    "missingLeft"
                } else {
                    "missingRight"
                },
                "Reference is missing after one origin fetch",
            )
            .side(side(index)));
        }
    }
    Ok([
        commits[0]
            .take()
            .ok_or_else(|| Problem::new("internal", "Left reference was not resolved"))?,
        commits[1]
            .take()
            .ok_or_else(|| Problem::new("internal", "Right reference was not resolved"))?,
    ])
}

async fn fetch_origin(context: &Context, job: &Job) -> Result<Option<Problem>, Problem> {
    match job.run(&context.root, FETCH_ARGS, &[0]).await {
        Ok(result) if result.code == Some(0) => Ok(None),
        Ok(result) => Ok(Some(Problem::new("networkError", &result.last_error()))),
        Err(problem) if problem.kind == "cancelled" => Err(problem),
        Err(problem) => Ok(Some(Problem::new("networkError", &problem.message))),
    }
}

fn side(index: usize) -> &'static str {
    if index == 0 {
        "left"
    } else {
        "right"
    }
}
