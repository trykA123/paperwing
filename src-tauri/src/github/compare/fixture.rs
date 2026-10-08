use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub(crate) struct Reply {
    pub status: u16,
    pub headers: String,
    pub body: Vec<u8>,
}

impl Reply {
    pub(crate) fn json(value: serde_json::Value) -> Self {
        Self {
            status: 200,
            headers: String::new(),
            body: serde_json::to_vec(&value).unwrap(),
        }
    }
}

pub(crate) struct Server {
    pub base: String,
    pub requests: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    pub(crate) async fn new(reply: impl Fn(&str, &str) -> Reply + Send + Sync + 'static) -> Self {
        let mut listener = None;
        for port in 42080..=42099 {
            if let Ok(bound) =
                tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await
            {
                listener = Some(bound);
                break;
            }
        }
        let listener = listener.expect("assigned fixture port available");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let address = base.clone();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                    let mut buffer = [0; 2048];
                    let length = socket.read(&mut buffer).await.unwrap();
                    if length == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..length]);
                    assert!(request.len() < 16384);
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request.starts_with("GET "));
                captured.lock().unwrap().push(request.clone());
                let response = reply(&request, &address);
                let header = format!(
                    "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n",
                    response.status,
                    response.body.len(),
                    response.headers
                );
                socket.write_all(header.as_bytes()).await.unwrap();
                let _ = socket.write_all(&response.body).await;
            }
        });
        Self {
            base,
            requests,
            task,
        }
    }

    pub(crate) fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(crate) fn source(host: &str) -> crate::settings::Source {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    serde_json::from_value(serde_json::json!({
        "id": format!("p40-{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
        "name": "admin", "kind": if host == "github.com" { "github" } else { "ghe" }, "host": host
    }))
    .unwrap()
}

pub(crate) fn page(files: usize, commits: usize) -> serde_json::Value {
    serde_json::json!({
        "base_commit": {"sha": "a".repeat(40)}, "merge_base_commit": {"sha": "c".repeat(40)},
        "ahead_by": 250, "behind_by": 3, "total_commits": 250,
        "commits": (0..commits).map(|i| serde_json::json!({"sha": format!("{i:040x}"), "commit": {"message": "fixture", "author": null}})).collect::<Vec<_>>(),
        "files": (0..files).map(|i| serde_json::json!({"sha": "d".repeat(40), "filename": format!("file-{i}.txt"), "status": "modified", "patch": "@@ -1 +1 @@\n-old\n+new"})).collect::<Vec<_>>()
    })
}
