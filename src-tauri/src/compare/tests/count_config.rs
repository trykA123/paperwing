use super::*;

#[tokio::test]
async fn automatic_eol_conversion_matches_storage_counts_for_cr_inputs() {
    let _guard = git::TEST_RUNNER_LOCK.lock().await;
    if std::env::var_os("SKEIN_COUNT_EOL_CHILD").is_some() {
        let fixture = Fixture::new().await;
        fixture.write("file", b"old\n");
        fixture.commit("base").await;
        fixture.git(&["config", "core.autocrlf", "false"]).await;
        #[cfg(target_os = "linux")]
        let storage = fixture.diff_data();
        #[cfg(not(target_os = "linux"))]
        let storage = crate::test_support::tmp_root();
        let eligibility = count_eligibility::Eligibility::new(storage);
        let mut job = fixture.job();
        job.rust_counts = eligibility
            .configuration(&fixture.0.join("repo"), &job)
            .await
            .unwrap();
        assert!(job.rust_counts.is_some());
        assert_eq!(
            line_counts(b"same\r\n", b"same\n", &job).await.unwrap(),
            legacy::counts(b"same\r\n", b"same\n", &job).await.unwrap()
        );
        return;
    }
    let fixture = Fixture::new().await;
    for setting in ["true", "input"] {
        let config = fixture.0.join(format!("global-{setting}"));
        std::fs::write(&config, format!("[core]\nautocrlf = {setting}\n")).unwrap();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "compare::tests::count_config::automatic_eol_conversion_matches_storage_counts_for_cr_inputs",
                "--nocapture",
            ])
            .env("SKEIN_COUNT_EOL_CHILD", "1")
            .env("GIT_CONFIG_GLOBAL", config);
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
