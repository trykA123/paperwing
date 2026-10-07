use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};

const MARKER: &str = "paperwing-disposable-profile-v1\n";
const IDENTIFIER: &str = "dev.paperwing.testing";
static ROOT: OnceLock<PathBuf> = OnceLock::new();

pub fn information(identifier: &str) {
    if std::env::args().any(|arg| arg == "--paperwing-test-profile-info") {
        println!("{}", serde_json::json!({ "profileBuild": "paperwing-test-profile-build-v1", "identifier": identifier, "benchmark": true }));
        std::process::exit(0);
    }
}

fn plain_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() || path.components().any(|part| matches!(part, Component::ParentDir)) {
        return Err("Test profile requires absolute contained directories".into());
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part);
        let metadata = std::fs::symlink_metadata(&current).map_err(|_| "Test profile directory missing")?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() { return Err("Test profile refuses links and non-directories".into()); }
    }
    std::fs::canonicalize(path).map_err(|_| "Test profile directory unavailable".into())
}

fn root(path: &Path) -> Result<PathBuf, String> {
    let root = plain_directory(path)?;
    let marker = root.join(".paperwing-disposable");
    let metadata = std::fs::symlink_metadata(&marker).map_err(|_| "Test profile marker missing")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || std::fs::read_to_string(marker).ok().as_deref() != Some(MARKER) {
        return Err("Test profile marker invalid".into());
    }
    Ok(root)
}

pub fn preflight(identifier: &str) -> Result<(), String> {
    if identifier != IDENTIFIER { return Err("Test-profile build requires dev.paperwing.testing configuration".into()); }
    let path = crate::env_names::var_os("SKEIN_TEST_PROFILE").ok_or("SKEIN_TEST_PROFILE is required")?;
    let path = root(Path::new(&path))?;
    #[cfg(target_os = "linux")]
    for (variable, suffix) in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")] {
        let actual = std::env::var_os(variable).ok_or("Test profile requires isolated XDG directories")?;
        if plain_directory(Path::new(&actual))? != path.join(suffix) { return Err("Test profile XDG directory mismatch".into()); }
    }
    ROOT.set(path).map_err(|_| "Test profile initialized twice".into())
}

pub fn ensure() -> Result<(), String> {
    let expected = ROOT.get().ok_or("Test profile has not passed isolation checks")?;
    if &root(expected)? != expected { return Err("Test profile root changed".into()); }
    for suffix in ["config/dev.paperwing.testing", "data/dev.paperwing.testing", "cache/dev.paperwing.testing", "data/dev.paperwing.testing/logs", "webview"] {
        if !plain_directory(&expected.join(suffix))?.starts_with(expected) { return Err("Test profile directory changed".into()); }
    }
    Ok(())
}

pub fn validate<R: tauri::Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    ensure()?;
    if app.config().identifier != IDENTIFIER || app.config().app.windows.iter().any(|window| window.create) {
        return Err("Test profile requires isolated config and deferred WebView creation".into());
    }
    let expected = ROOT.get().ok_or("Test profile unavailable")?;
    let resolver = app.path();
    for path in [resolver.app_config_dir(), resolver.app_data_dir(), resolver.app_local_data_dir(), resolver.app_cache_dir(), resolver.app_log_dir()] {
        let actual = plain_directory(&path.map_err(|_| "Test app directory could not resolve")?)?;
        if !actual.starts_with(expected) || actual == *expected { return Err("Resolved app directory escapes test profile".into()); }
    }
    let sample = sample()?;
    plain_directory(&expected.join("webview").join(format!("sample-{sample}")))
}

pub fn source(id: &str) -> Result<(), String> {
    ensure()?;
    let suffix = id.strip_prefix("fixture-").ok_or("Test profile requires generated fixture source IDs")?;
    if suffix.len() != 36 || suffix.chars().enumerate().any(|(index, value)| {
        if [8, 13, 18, 23].contains(&index) { value != '-' } else { !value.is_ascii_hexdigit() }
    }) { return Err("Test profile requires generated fixture source IDs".into()); }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn root_probe_delay(path: &str) -> Result<(), String> {
    let Ok(value) = crate::env_names::var("SKEIN_TEST_ROOT_PROBE_DELAY_MS") else { return Ok(()); };
    let delay: u64 = value.parse().map_err(|_| "Invalid isolated root-probe delay")?;
    if !(100..=2000).contains(&delay) { return Err("Isolated root-probe delay exceeds its bound".into()); }
    ensure()?;
    let file = ROOT.get().ok_or("Test profile unavailable")?.join("config/dev.paperwing.testing/settings.json");
    plain_file(&file)?;
    let saved: crate::settings::Settings = serde_json::from_slice(&std::fs::read(file).map_err(|_| "Test profile settings unavailable")?)
        .map_err(|_| "Invalid isolated root-probe settings")?;
    settings(&saved)?;
    if saved.workspace.get("root").and_then(serde_json::Value::as_str) != Some(path) {
        return Err("Root-probe delay requires the saved disposable workspace".into());
    }
    std::thread::sleep(std::time::Duration::from_millis(delay));
    Ok(())
}

pub fn github_endpoint() -> Result<Option<String>, String> {
    ensure()?;
    let Ok(value) = crate::env_names::var("SKEIN_TEST_GITHUB_API") else { return Ok(None); };
    let url = reqwest::Url::parse(&value).map_err(|_| "Invalid isolated GitHub API endpoint")?;
    if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") || url.port() != Some(5951)
        || !url.username().is_empty() || url.password().is_some() || url.path() != "/"
        || url.query().is_some() || url.fragment().is_some() {
        return Err("GitHub API fixture must use the isolated loopback port".into());
    }
    Ok(Some(value.trim_end_matches('/').into()))
}

pub fn settings(settings: &crate::settings::Settings) -> Result<(), String> {
    ensure()?;
    for item in &settings.sources {
        source(&item.id)?;
        if item.kind != "manual" || !item.urls.is_empty() || !item.host.is_empty() || !item.orgs.is_empty() {
            return Err("Test profile permits local manual fixtures only".into());
        }
    }
    let workspace = settings.workspace.get("root").and_then(serde_json::Value::as_str)
        .ok_or("Test profile workspace root missing")?;
    let workspace = plain_directory(Path::new(workspace))?;
    if workspace.file_name().and_then(|part| part.to_str()) != Some("checkouts") {
        return Err("Test profile workspace must use fixture checkouts".into());
    }
    let parent = workspace.parent().ok_or("Fixture root missing")?;
    let marker = parent.join(".paperwing-disposable");
    if std::fs::symlink_metadata(&marker).map_err(|_| "Fixture marker missing")?.file_type().is_symlink()
        || std::fs::read_to_string(marker).ok().as_deref() != Some("paperwing-disposable-fixture-v1\n") {
        return Err("Test profile workspace fixture marker invalid".into());
    }
    Ok(())
}

pub fn plain_file(path: &Path) -> Result<(), String> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.is_file() || metadata.file_type().is_symlink() { return Err("Test profile refuses linked files".into()); }
    }
    Ok(())
}

fn sample() -> Result<u32, String> {
    let sample: u32 = crate::env_names::var("SKEIN_BENCHMARK_SAMPLE").map_err(|_| "Benchmark sample required")?
        .parse().map_err(|_| "Invalid benchmark sample")?;
    if sample == 0 { return Err("Benchmark sample must be positive".into()); }
    Ok(sample)
}

pub fn trace() -> Result<(PathBuf, u32), String> {
    ensure()?;
    let sample = sample()?;
    let root = ROOT.get().ok_or("Test profile unavailable")?;
    let logs = plain_directory(&root.join("data/dev.paperwing.testing/logs"))?;
    Ok((logs.join(format!("sample-{sample}.jsonl")), sample))
}

#[tauri::command]
pub fn benchmark_plan() -> Result<serde_json::Value, String> {
    ensure()?;
    let path = ROOT.get().ok_or("Test profile unavailable")?.join("benchmark-plan.json");
    if std::fs::symlink_metadata(&path).map_err(|_| "Benchmark plan missing")?.file_type().is_symlink() {
        return Err("Benchmark plan cannot be linked".into());
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|_| "Benchmark plan unavailable")?)
        .map_err(|_| "Benchmark plan invalid".into())
}

#[tauri::command]
pub fn benchmark_finish(app: AppHandle, fingerprint: String, proof: Option<serde_json::Value>) -> Result<(), String> {
    validate(&app)?;
    if fingerprint.len() != 64 || !fingerprint.chars().all(|value| value.is_ascii_hexdigit()) {
        return Err("Invalid result fingerprint".into());
    }
    if let Some(proof) = &proof {
        let valid = proof["platform"] == "linux" && match proof["scenario"].as_str() {
            Some("linux-read-only") => proof["connected"] == true && proof["readOnly"] == true
                && proof["originalEditable"] == false && proof["ticketCount"] == 0
                && proof["workingSide"] == "right" && proof["refusals"].as_array().map(Vec::len) == Some(10),
            Some("linux-root-probe") => proof["expectedResponsive"].is_boolean()
                && proof["fastBeforeProbeDone"] == proof["expectedResponsive"] && proof["rootValid"] == true
                && proof["frames"].as_u64().is_some_and(|frames| proof["expectedResponsive"] == false || frames >= 2)
                && proof["elapsedMs"].as_f64().is_some_and(|elapsed| elapsed.is_finite() && elapsed >= 900.0),
            _ => false,
        };
        if !valid { return Err("Invalid native profile proof".into()); }
    }
    let (trace, _) = trace()?;
    let output = std::fs::OpenOptions::new().create_new(true).write(true).open(trace.with_extension("result.json"))
        .map_err(|_| "Result already exists or cannot be created")?;
    let snapshot = crate::benchmark::benchmark_snapshot()?;
    serde_json::to_writer(output, &serde_json::json!({ "resultFingerprint": fingerprint, "commands": snapshot["commands"], "droppedEvents": snapshot["droppedEvents"], "writeFailures": snapshot["writeFailures"], "proof": proof }))
        .map_err(|_| "Benchmark result could not be recorded")?;
    app.exit(0);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_refuse_unmarked_and_linked_roots_before_access() {
        let path = std::env::temp_dir().join(format!("skein-profile-refusal-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        assert!(root(&path).is_err());
        std::fs::write(path.join(".paperwing-disposable"), MARKER).unwrap();
        assert_eq!(root(&path).unwrap(), std::fs::canonicalize(&path).unwrap());
        assert!(plain_directory(&path.join("../outside")).is_err());
        #[cfg(unix)] {
            let link = path.join("link");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(root(&link).is_err());
        }
        assert!(preflight("dev.paperwing.app").is_err());
        assert!(source("real-source").is_err());
    }
}
