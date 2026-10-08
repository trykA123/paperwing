use super::*;
use crate::github::compare::fixture::{page, source, Reply, Server};
use crate::github::compare::Binding;

#[tokio::test]
async fn existing_local_refs_stay_local_and_missing_refs_require_explicit_github() {
    let _guard = crate::git::TEST_RUNNER_LOCK.lock().await;
    let fixture = Fixture::new().await;
    fixture.write("file.txt", b"local\n");
    let local_sha = fixture.commit("local").await;
    let source = source("gitext.company.com");
    let url = format!("https://{}/admin/repo.git", source.host);
    fixture.git(&["remote", "add", "origin", &url]).await;
    let server = Server::new(|request, _| {
        if request.contains("/compare/") {
            Reply::json(page(1, 1))
        } else {
            Reply::json(serde_json::json!({"sha": if request.contains("heads%2Fmain") { "a".repeat(40) } else { "b".repeat(40) }}))
        }
    }).await;
    let _binding = Binding::new(&source.id, &server.base);
    let service = fixture.service();
    service.configure_remote(fixture.0.join("cache")).unwrap();
    let mut settings = fixture.settings();
    settings.workspace["sets"][0]["items"][0]["repoId"] =
        format!("{}:admin/repo", source.id).into();
    settings.workspace["sets"][0]["items"][0]["url"] = url.into();
    settings.sources.push(source);
    let main = CompareRef::Branch {
        name: "main".into(),
    };
    let contexts = [fixture.context(main.clone()), fixture.context(main.clone())];
    let opened = service
        .open(
            &settings,
            contexts[0].endpoint.clone(),
            contexts[1].endpoint.clone(),
        )
        .await
        .unwrap();
    fixture.git(&["remote", "remove", "origin"]).await;
    for source in [
        None,
        Some(CompareSource::Github),
        Some(CompareSource::Local),
    ] {
        let RefreshResult::Ready { snapshot } = service
            .refresh_with_source(&settings, &opened.id, Options::default(), source)
            .await
            .unwrap()
        else {
            panic!("local comparison unavailable");
        };
        assert_eq!(snapshot.source, "local");
        assert_eq!(snapshot.left.commit, local_sha);
    }
    service.close(&opened.id).await;
    fixture
        .git(&[
            "remote",
            "add",
            "origin",
            &format!("https://{}/admin/repo.git", settings.sources[0].host),
        ])
        .await;
    let missing = fixture.context(CompareRef::Branch {
        name: "topic".into(),
    });
    let contexts = [contexts[0].clone(), missing];
    assert!(
        github_source::choose(&settings, &contexts, None, &fixture.job())
            .await
            .unwrap()
            .is_none()
    );
    assert!(github_source::choose(
        &settings,
        &contexts,
        Some(CompareSource::Local),
        &fixture.job()
    )
    .await
    .unwrap()
    .is_none());
    assert_eq!(server.count(), 0);
    let opened = service
        .open(
            &settings,
            contexts[0].endpoint.clone(),
            contexts[1].endpoint.clone(),
        )
        .await
        .unwrap();
    let RefreshResult::Ready { snapshot } = service
        .refresh_with_source(
            &settings,
            &opened.id,
            Options::default(),
            Some(CompareSource::Github),
        )
        .await
        .unwrap()
    else {
        panic!("explicit remote comparison unavailable");
    };
    assert_eq!(snapshot.source, "github");
    assert_eq!(server.count(), 3);
    service.close(&opened.id).await;
}
