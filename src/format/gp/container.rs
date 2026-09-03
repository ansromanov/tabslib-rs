//! The container half of the `.gp` adapter: a zip whose payload is a single
//! XML document.

use crate::error::{Error, Result};

fn wrap<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::format("gp", e)
}
use std::io::{Cursor, Read, Write};

const GPIF_PATH: &str = "Content/score.gpif";

/// Extracts the GPIF payload from GP7/8 container bytes.
pub(crate) fn read_payload(bytes: &[u8]) -> Result<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(wrap)?;
    let names: Vec<String> = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();
    let target = names
        .iter()
        .find(|n| n.ends_with("score.gpif"))
        .ok_or_else(|| Error::Malformed("no score payload in the archive".into()))?
        .clone();
    let mut file = zip.by_name(&target).map_err(wrap)?;
    let mut out = String::new();
    file.read_to_string(&mut out)?;
    Ok(out)
}

/// Writes a container with a fixed timestamp so output is byte-deterministic --
/// the zip writer stamps wall-clock time otherwise, which has broken
/// reproducibility in this project before.
pub(crate) fn write_payload(xml: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::from_date_and_time(2020, 1, 1, 0, 0, 0).unwrap());
        zip.start_file(GPIF_PATH, opts).map_err(wrap)?;
        zip.write_all(xml.as_bytes())?;
        zip.finish().map_err(wrap)?;
    }
    Ok(buf)
}
