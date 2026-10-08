use super::{Error, Http, Response};
use crate::github::blob::{Blob, MAX_BYTES};
use reqwest::Method;

impl Http<'_> {
    pub(crate) async fn raw_blob(&self, path: &str) -> Result<Blob, Error> {
        let mut request = self.build_request(Method::GET, path, None)?;
        request.headers_mut().insert(
            "accept",
            reqwest::header::HeaderValue::from_static("application/vnd.github.raw"),
        );
        let lease =
            crate::providers::acquire(self.source, &self.api_host, None).map_err(Error::Message)?;
        let response = lease
            .run(self.dispatch_request(request, |request| async {
                let response = self
                    .client
                    .execute(request)
                    .await
                    .map_err(|_| self.connection_error())?;
                Ok((response.status().as_u16(), response))
            }))
            .await
            .map_err(|error| Error::Message(error.to_string()))??;
        let metadata = Response {
            status: response.status().as_u16(),
            headers: response.headers().clone(),
            body: Vec::new(),
            next: false,
        };
        metadata.check_rate_limit()?;
        if !response.status().is_success() {
            let response = self
                .read_response(&format!("{}{path}", self.base), response)
                .await?;
            response.check_rate_limit()?;
            return response.decode::<Blob>().map(|page| page.data);
        }
        self.read_blob(response).await
    }

    async fn read_blob(&self, mut response: reqwest::Response) -> Result<Blob, Error> {
        let size = response.content_length();
        if size.is_some_and(|size| size > MAX_BYTES as u64) {
            return Ok(Blob {
                bytes: None,
                size,
                binary: false,
            });
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| Error::Message("Cannot read GitHub blob".into()))?
        {
            self.check_revision()?;
            if body.len() + chunk.len() > MAX_BYTES {
                return Ok(Blob {
                    bytes: None,
                    size: size.or(Some((body.len() + chunk.len()) as u64)),
                    binary: false,
                });
            }
            if chunk.contains(&0) {
                return Ok(Blob {
                    bytes: None,
                    size,
                    binary: true,
                });
            }
            body.extend_from_slice(&chunk);
        }
        self.check_revision()?;
        let binary = std::str::from_utf8(&body).is_err();
        Ok(Blob {
            size: Some(body.len() as u64),
            bytes: (!binary).then_some(body),
            binary,
        })
    }
}
