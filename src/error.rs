use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("not a Guitar Pro 7/8 container: no Content/score.gpif")]
    NotGp7,
    #[error("malformed gpif: {0}")]
    Malformed(String),
}

pub type Result<T> = std::result::Result<T, Error>;
