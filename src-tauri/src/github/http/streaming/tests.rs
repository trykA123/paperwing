use super::*;
use crate::github::http::Connection;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const BODY_BYTES: usize = 70 * 1024 * 1024;

fn scratch() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = crate::test_support::tmp_root().join(format!(
        "stream-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn byte_at(index: usize) -> u8 {
    (index % 251) as u8
}

async fn serve(status: &'static str, announced: usize, sent: usize, stall: bool) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 4096];
        let _ = socket.read(&mut request).await;
        let head = format!(
            "HTTP/1.1 {status}\r\ncontent-length: {announced}\r\nconnection: close\r\n\r\n"
        );
        socket.write_all(head.as_bytes()).await.unwrap();
        let mut written = 0;
        while written < sent {
            let size = (sent - written).min(256 * 1024);
            let chunk: Vec<u8> = (written..written + size).map(byte_at).collect();
            if socket.write_all(&chunk).await.is_err() {
                return;
            }
            written += size;
        }
        if stall {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });
    port
}

async fn connect(source: &crate::settings::Source, port: u16) -> Http<'_> {
    Http::with_token(
        Connection {
            source,
            base: format!("http://127.0.0.1:{port}"),
            host: &source.host,
            expected: 0,
        },
        async { Ok(Some("synthetic-ci-token".into())) },
        || 0,
    )
    .await
    .unwrap()
}

fn source(id: &str) -> crate::settings::Source {
    serde_json::from_value(
        serde_json::json!({"id":id,"name":"admin","kind":"ghe","host":"enterprise.invalid"}),
    )
    .unwrap()
}

fn leftovers(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[tokio::test]
async fn streamed_download_larger_than_the_text_limit_lands_on_disk_intact() {
    let dir = scratch();
    let source = source("stream-large-source");
    let http = connect(
        &source,
        serve("200 OK", BODY_BYTES, BODY_BYTES, false).await,
    )
    .await;
    let destination = dir.join("artifact.zip");
    let response = http.stream_download("/zip", &destination).await.unwrap();
    assert_eq!(response.status, 200);
    assert!(response.body.is_empty());
    assert_eq!(leftovers(&dir), ["artifact.zip"]);
    let mut file = tokio::fs::File::open(&destination).await.unwrap();
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut offset = 0;
    loop {
        let read = file.read(&mut buffer).await.unwrap();
        if read == 0 {
            break;
        }
        assert!(buffer[..read]
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte == byte_at(offset + index)));
        offset += read;
    }
    assert_eq!(offset, BODY_BYTES);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn failed_download_leaves_no_partial_file_and_keeps_the_existing_destination() {
    let dir = scratch();
    let source = source("stream-failed-source");
    let destination = dir.join("artifact.zip");
    std::fs::write(&destination, b"previous").unwrap();
    let http = connect(
        &source,
        serve("200 OK", 10 * 1024 * 1024, 3 * 1024 * 1024, false).await,
    )
    .await;
    let error = http
        .stream_download("/zip", &destination)
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("Cannot read CI download"));
    assert_eq!(leftovers(&dir), ["artifact.zip"]);
    assert_eq!(std::fs::read(&destination).unwrap(), b"previous");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn cancelled_download_leaves_no_partial_file() {
    let dir = scratch();
    let source = source("stream-cancelled-source");
    let http = connect(
        &source,
        serve("200 OK", 10 * 1024 * 1024, 1024 * 1024, true).await,
    )
    .await;
    let destination = dir.join("artifact.zip");
    let attempt = tokio::time::timeout(
        Duration::from_millis(700),
        http.stream_download("/zip", &destination),
    )
    .await;
    assert!(attempt.is_err());
    assert!(leftovers(&dir).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn http_failure_writes_nothing_and_reports_the_status() {
    let dir = scratch();
    let source = source("stream-status-source");
    let http = connect(&source, serve("410 Gone", 0, 0, false).await).await;
    let response = http
        .stream_download("/zip", &dir.join("artifact.zip"))
        .await
        .unwrap();
    assert!(matches!(
        response.checked(),
        Err(Error::Http { status: 410, .. })
    ));
    assert!(leftovers(&dir).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn destination_must_be_absolute_with_an_existing_folder_and_not_a_directory() {
    let dir = scratch();
    std::fs::create_dir(dir.join("folder")).unwrap();
    for bad in [
        PathBuf::from("relative.zip"),
        dir.join("missing").join("artifact.zip"),
        dir.join("folder"),
        PathBuf::from("/"),
    ] {
        assert!(validate_destination(&bad).await.is_err(), "{bad:?}");
    }
    validate_destination(&dir.join("artifact.zip"))
        .await
        .unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}
