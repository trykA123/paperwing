use super::*;

impl Service {
    pub(super) async fn resolve_comparison(
        &self,
        contexts: [Context; 2],
        job: &Job,
    ) -> Result<(Resolved, Resolved, Job), Problem> {
        let safes = [
            read_root(&contexts[0], job)
                .await
                .map_err(|problem| problem.side("left"))?,
            read_root(&contexts[1], job)
                .await
                .map_err(|problem| problem.side("right"))?,
        ];
        let job = self.configure_counts(&contexts, &safes, job).await?;
        let mut states = Vec::new();
        let mut epochs = Vec::new();
        for safe in &safes {
            let state = self.fetch_state(&safe.path).await?;
            epochs.push(job.lock(&state).await?.epoch);
            states.push(state);
        }
        let configs = [
            self.read_diff_config(&contexts[0], &job).await?,
            self.read_diff_config(&contexts[1], &job).await?,
        ];
        let commits = super::ref_resolution::resolve_refs(super::ref_resolution::References {
            contexts: contexts.each_ref(),
            roots: safes.each_ref().map(|safe| &safe.path),
            states,
            epochs,
            job: &job,
        })
        .await?;
        let formats = [
            ObjectFormat::read(&contexts[0].root, &job).await?,
            ObjectFormat::read(&contexts[1].root, &job).await?,
        ];
        let readers = register_readers(&contexts, &job).await?;
        let mut resolved = contexts
            .into_iter()
            .zip(safes)
            .zip(commits)
            .zip(configs)
            .zip(formats)
            .zip(readers)
            .map(
                |(((((context, safe), commit), diff_config), object_format), reader)| Resolved {
                    context,
                    safe,
                    commit,
                    diff_config,
                    object_format,
                    reader,
                    files: BTreeMap::new(),
                },
            );
        let left = resolved
            .next()
            .ok_or_else(|| Problem::new("internal", "Left endpoint unavailable"))?;
        let right = resolved
            .next()
            .ok_or_else(|| Problem::new("internal", "Right endpoint unavailable"))?;
        Ok((left, right, job))
    }

    async fn configure_counts(
        &self,
        contexts: &[Context; 2],
        safes: &[paths::ReadRoot; 2],
        job: &Job,
    ) -> Result<Job, Problem> {
        let mut job = job.clone();
        #[cfg(target_os = "linux")]
        if let Some(storage) = &job.diff {
            job.roots = storage
                .capture(safes.to_vec(), &job.cancel)
                .await
                .map_err(|error| {
                    Problem::new(
                        if error.cancelled {
                            "cancelled"
                        } else {
                            "unavailable"
                        },
                        error.message,
                    )
                })?;
        }
        #[cfg(not(target_os = "linux"))]
        let _ = safes;
        let eligibility = self.counts.get_or_init(|| {
            #[cfg(target_os = "linux")]
            let eligibility = count_eligibility::Eligibility::default();
            #[cfg(not(target_os = "linux"))]
            let eligibility = count_eligibility::Eligibility::new(std::env::temp_dir());
            Arc::new(eligibility)
        });
        let fallback;
        let eligibility = if job
            .count_root
            .as_ref()
            .is_some_and(|root| Some(root) != eligibility.storage.as_ref())
        {
            fallback = count_eligibility::Eligibility::new(
                job.count_root
                    .clone()
                    .ok_or_else(|| Problem::new("unavailable", "Count storage unavailable"))?,
            );
            &fallback
        } else {
            eligibility.as_ref()
        };
        let left = git::enrichment(eligibility.configuration(&contexts[0].root, &job)).await?;
        let right = git::enrichment(eligibility.configuration(&contexts[1].root, &job)).await?;
        job.rust_counts = left.filter(|mode| Some(*mode) == right);
        Ok(job)
    }

    async fn read_diff_config(&self, context: &Context, job: &Job) -> Result<Vec<String>, Problem> {
        let config = {
            let mut configs = self.diff_configs();
            if configs.len() >= 32 {
                configs.retain(|_, config| Arc::strong_count(config) > 1);
            }
            configs.entry(context.root.clone()).or_default().clone()
        };
        config
            .get_or_try_init(|| {
                git::enrichment(async {
                    let result = job
                        .run(
                            &context.root,
                            &["config", "--get-regexp", "^diff\\.(algorithm|renamelimit)$"],
                            &[0, 1],
                        )
                        .await?;
                    let mut values = Vec::new();
                    for line in decode(&result.stdout)?.lines() {
                        if let Some((key, value)) = line.split_once(' ') {
                            values.extend(["-c".into(), format!("{key}={value}")]);
                        }
                    }
                    Ok::<_, Problem>(values)
                })
            })
            .await
            .cloned()
    }
}

async fn register_readers(
    contexts: &[Context; 2],
    job: &Job,
) -> Result<[git::BatchReader; 2], Problem> {
    let left = git::BatchReader::new(contexts[0].root.clone(), job.cancel.clone());
    let right = if contexts[0].root == contexts[1].root
        || left.shares_directory(contexts[1].root.clone()).await
    {
        left.clone()
    } else {
        git::BatchReader::new(contexts[1].root.clone(), job.cancel.clone())
    };
    let mut readers = job.readers.lock().await;
    job.check()?;
    readers.extend([left.clone(), right.clone()]);
    Ok([left, right])
}
