use super::{Job, Problem};
use sha1::Digest;
use std::path::Path;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum ObjectFormat {
    Sha1,
    Sha256,
}

impl ObjectFormat {
    pub(super) async fn read(root: &Path, job: &Job) -> Result<Self, Problem> {
        match job
            .output(root, &["rev-parse", "--show-object-format"])
            .await?
            .as_slice()
        {
            b"sha1\n" | b"sha1\r\n" => Ok(Self::Sha1),
            b"sha256\n" | b"sha256\r\n" => Ok(Self::Sha256),
            _ => Err(Problem::new("gitError", "Unsupported Git object format")),
        }
    }

    pub(super) fn blob(self, bytes: &[u8]) -> String {
        let header = format!("blob {}\0", bytes.len());
        match self {
            Self::Sha1 => {
                let mut hash = sha1::Sha1::new();
                hash.update(header.as_bytes());
                hash.update(bytes);
                format!("{:x}", hash.finalize())
            }
            Self::Sha256 => {
                let mut hash = sha2::Sha256::new();
                hash.update(header.as_bytes());
                hash.update(bytes);
                format!("{:x}", hash.finalize())
            }
        }
    }
}
