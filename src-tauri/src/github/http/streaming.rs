use super::{transfers::Hop, Error, Http, Response};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::{AsyncWriteExt, BufWriter};

const PART_SUFFIX: &str = ".skein-part";
const WRITE_BUFFER: usize = 64 * 1024;
const CHUNK_IDLE: Duration = Duration::from_secs(60);
const TOTAL_TIME: Duration = Duration::from_secs(6 * 60 * 60);
const ERROR_BODY_LIMIT: usize = 64 * 1024;

async fn validate_destination(destination: &Path) -> Result<(), Error> {
    let invalid = |reason: &str| Error::Message(format!("Invalid download destination: {reason}"));
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("missing file name"))?;
    if !destination.is_absolute() || name.chars().any(char::is_control) {
        return Err(invalid("use an absolute file path"));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("missing folder"))?;
    match tokio::fs::metadata(parent).await {
        Ok(metadata) if metadata.is_dir() => {}
        _ => return Err(invalid("the folder does not exist")),
    }
    if tokio::fs::metadata(destination)
        .await
        .is_ok_and(|metadata| metadata.is_dir())
    {
        return Err(invalid("the path is a folder"));
    }
    Ok(())
}

fn part_path(destination: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut name = destination.as_os_str().to_os_string();
    name.push(format!(".{}-{nanos}{PART_SUFFIX}", std::process::id()));
    PathBuf::from(name)
}

struct PartFile(Option<PathBuf>);

impl Drop for PartFile {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl Http<'_> {
    pub(crate) async fn download_to(
        &self,
        path: &str,
        destination: &Path,
    ) -> Result<Response, Error> {
        validate_destination(destination).await?;
        let lease =
            crate::providers::acquire(self.source, &self.api_host, None).map_err(Error::Message)?;
        lease
            .run(self.stream_download(path, destination))
            .await
            .map_err(|error| Error::Message(error.to_string()))?
    }

    pub(super) async fn stream_download(
        &self,
        path: &str,
        destination: &Path,
    ) -> Result<Response, Error> {
        let mut response = self
            .download_with(path, |mut request| {
                *request.timeout_mut() = Some(TOTAL_TIME);
                Box::pin(async {
                    self.client
                        .execute(request)
                        .await
                        .map_err(|_| Error::Message("Cannot download CI content".into()))
                })
            })
            .await?;
        if !(200..300).contains(&Hop::status(&response)) {
            let url = response.url().to_string();
            return self.read_bounded(&url, response, ERROR_BODY_LIMIT).await;
        }
        let status = Hop::status(&response);
        let headers = response.headers().clone();
        let part = part_path(destination);
        let mut guard = PartFile(None);
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part)
            .await
            .map_err(|_| Error::Message("Cannot create the download file".into()))?;
        guard.0 = Some(part.clone());
        let mut out = BufWriter::with_capacity(WRITE_BUFFER, file);
        loop {
            let chunk = tokio::time::timeout(CHUNK_IDLE, response.chunk())
                .await
                .map_err(|_| Error::Message("CI download stalled".into()))?
                .map_err(|_| Error::Message("Cannot read CI download".into()))?;
            let Some(chunk) = chunk else { break };
            out.write_all(&chunk)
                .await
                .map_err(|_| Error::Message("Cannot write the download file".into()))?;
        }
        out.flush()
            .await
            .map_err(|_| Error::Message("Cannot write the download file".into()))?;
        out.get_ref()
            .sync_all()
            .await
            .map_err(|_| Error::Message("Cannot write the download file".into()))?;
        drop(out);
        self.check_revision()?;
        tokio::fs::rename(&part, destination)
            .await
            .map_err(|_| Error::Message("Cannot save the download file".into()))?;
        guard.0 = None;
        Ok(Response {
            status,
            headers,
            body: Vec::new(),
            next: false,
        })
    }
}

#[cfg(test)]
mod tests;
