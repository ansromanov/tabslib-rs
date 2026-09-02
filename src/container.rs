//! GP7/GP8 container: a zip whose payload is `Content/score.gpif`.

use std::io::{Cursor, Read, Write};
use crate::error::{Error, Result};

const GPIF_PATH: &str = "Content/score.gpif";

pub fn read_gpif(bytes: &[u8]) -> Result<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    let names: Vec<String> = (0..zip.len())
        .filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();
    let target = names
        .iter()
        .find(|n| n.ends_with("score.gpif"))
        .ok_or(Error::NotGp7)?
        .clone();
    let mut file = zip.by_name(&target)?;
    let mut out = String::new();
    file.read_to_string(&mut out)?;
    Ok(out)
}

/// Writes a container with a fixed timestamp so output is byte-deterministic --
/// the zip writer stamps wall-clock time otherwise, which has broken
/// reproducibility in this project before.
pub fn write_gpif(xml: &str) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::from_date_and_time(2020, 1, 1, 0, 0, 0).unwrap());
        zip.start_file(GPIF_PATH, opts)?;
        zip.write_all(xml.as_bytes())?;
        zip.finish()?;
    }
    Ok(buf)
}
