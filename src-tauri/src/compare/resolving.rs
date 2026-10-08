use super::*;

impl Service {
    pub(super) async fn resolve_comparison(
        &self,
        contexts: [Context; 2],
        job: &Job,
    ) -> Result<(Resolved, Resolved, Job), Problem> {
        let [left_context, right_context] = contexts;
        let left_safe = read_root(&left_context, job)
            .await
            .map_err(|problem| problem.side("left"))?;
        let right_safe = read_root(&right_context, job)
            .await
            .map_err(|problem| problem.side("right"))?;
        #[cfg(target_os = "linux")]
        let storage_job = {
            let mut captured = job.clone();
            if let Some(storage) = &captured.diff {
                captured.roots = storage
                    .capture(
                        vec![left_safe.clone(), right_safe.clone()],
                        &captured.cancel,
                    )
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
            captured
        };
        #[cfg(target_os = "linux")]
        let job = &storage_job;
        let mut count_job = job.clone();
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
        let left_counts = eligibility.configuration(&left_context.root, job).await?;
        let right_counts = eligibility.configuration(&right_context.root, job).await?;
        count_job.rust_counts = left_counts.filter(|mode| Some(*mode) == right_counts);
        let job = &count_job;
        let contexts = [&left_context, &right_context];
        let roots = [&left_safe.path, &right_safe.path];
        let mut states = Vec::new();
        let mut epochs = Vec::new();
        for root in roots {
            let state = self.fetch_state(root).await?;
            epochs.push(job.lock(&state).await?.epoch);
            states.push(state);
        }
        let mut diff_configs = Vec::new();
        for context in contexts {
            let config = {
                let mut configs = self.diff_configs();
                if configs.len() >= 32 {
                    configs.retain(|_, config| Arc::strong_count(config) > 1);
                }
                configs.entry(context.root.clone()).or_default().clone()
            };
            let values = config
                .get_or_try_init(|| async {
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
                .await?;
            diff_configs.push(values.clone());
        }
        let mut commits = Vec::new();
        for (index, context) in contexts.iter().enumerate() {
            commits.push(
                resolve(context, job)
                    .await
                    .map_err(|problem| problem.side(if index == 0 { "left" } else { "right" }))?,
            );
        }
        let mut attempted = BTreeSet::new();
        for index in 0..2 {
            if commits[index].is_some() {
                continue;
            }
            let side = if index == 0 { "left" } else { "right" };
            let mut state = job.lock(&states[index]).await?;
            if state.epoch == epochs[index]
                && attempted.insert(roots[index].clone())
                && resolve(contexts[index], job).await?.is_none()
            {
                let result = job
                    .run(
                        &contexts[index].root,
                        &[
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
                        ],
                        &[0],
                    )
                    .await;
                state.epoch += 1;
                state.problem = match result {
                    Ok(result) if result.code == Some(0) => None,
                    Ok(result) => Some(Problem::new("networkError", &result.last_error())),
                    Err(problem) if problem.kind == "cancelled" => return Err(problem),
                    Err(problem) => Some(Problem::new("networkError", &problem.message)),
                };
            }
            commits[index] = resolve(contexts[index], job)
                .await
                .map_err(|problem| problem.side(side))?;
            if commits[index].is_none() {
                if let Some(problem) = &state.problem {
                    return Err(problem.clone().side(side));
                }
                return Err(Problem::new(
                    if index == 0 {
                        "missingLeft"
                    } else {
                        "missingRight"
                    },
                    "Reference is missing after one origin fetch",
                )
                .side(side));
            }
        }
        let left_format = ObjectFormat::read(&left_context.root, job).await?;
        let right_format = ObjectFormat::read(&right_context.root, job).await?;
        let left_reader = git::BatchReader::new(left_context.root.clone(), job.cancel.clone());
        let right_reader = if left_context.root == right_context.root
            || left_reader
                .shares_directory(right_context.root.clone())
                .await
        {
            left_reader.clone()
        } else {
            git::BatchReader::new(right_context.root.clone(), job.cancel.clone())
        };
        {
            let mut readers = job.readers.lock().await;
            job.check()?;
            readers.extend([left_reader.clone(), right_reader.clone()]);
        }
        let left = Resolved {
            object_format: left_format,
            diff_config: diff_configs[0].clone(),
            reader: left_reader,
            context: left_context,
            safe: left_safe,
            commit: commits[0].take().unwrap(),
            files: BTreeMap::new(),
        };
        let right = Resolved {
            object_format: right_format,
            diff_config: diff_configs[1].clone(),
            reader: right_reader,
            context: right_context,
            safe: right_safe,
            commit: commits[1].take().unwrap(),
            files: BTreeMap::new(),
        };
        Ok((left, right, job.clone()))
    }
}
