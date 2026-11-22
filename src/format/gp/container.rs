//! The container half of the `.gp` adapter: a zip whose payload is a single
//! XML document.

use crate::error::{Error, Result};

fn wrap<E: std::error::Error + Send + Sync + 'static>(e: E) -> Error {
    Error::format("gp", e)
}
use std::io::{Cursor, Read, Write};

const GPIF_PATH: &str = "Content/score.gpif";
type RetainedEntries = Vec<(String, Vec<u8>)>;

/// Limits applied while opening a GP7/8 container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerLimits {
    /// Maximum input archive size in bytes.
    pub max_archive_bytes: usize,
    /// Maximum number of archive entries.
    pub max_entries: usize,
    /// Maximum decompressed size of one entry in bytes.
    pub max_entry_bytes: usize,
    /// Maximum decompressed size of all entries in bytes.
    pub max_total_uncompressed_bytes: usize,
    /// Maximum decompressed-to-compressed size ratio.
    pub max_compression_ratio: usize,
}

/// Conservative defaults for untrusted GP7/8 input.
pub const DEFAULT_CONTAINER_LIMITS: ContainerLimits = ContainerLimits {
    max_archive_bytes: 128 * 1024 * 1024,
    max_entries: 4096,
    max_entry_bytes: 64 * 1024 * 1024,
    max_total_uncompressed_bytes: 256 * 1024 * 1024,
    max_compression_ratio: 1000,
};

fn checked_limit(limit: usize, resource: &'static str) -> Result<usize> {
    if limit == 0 {
        Err(Error::LimitExceeded { resource, limit })
    } else {
        Ok(limit)
    }
}

fn validate_limits(limits: ContainerLimits) -> Result<()> {
    checked_limit(limits.max_archive_bytes, "GP archive bytes")?;
    checked_limit(limits.max_entries, "GP archive entries")?;
    checked_limit(limits.max_entry_bytes, "GP entry")?;
    checked_limit(
        limits.max_total_uncompressed_bytes,
        "GP total uncompressed bytes",
    )?;
    checked_limit(limits.max_compression_ratio, "GP compression ratio")?;
    Ok(())
}

fn read_entry(
    file: &mut zip::read::ZipFile<'_>,
    limits: ContainerLimits,
    total: &mut usize,
) -> Result<Vec<u8>> {
    let entry_limit = checked_limit(limits.max_entry_bytes, "GP entry")?;
    let mut data = Vec::new();
    let read_limit = entry_limit.checked_add(1).ok_or(Error::LimitExceeded {
        resource: "GP entry",
        limit: entry_limit,
    })?;
    let read_limit = u64::try_from(read_limit).map_err(|_| Error::LimitExceeded {
        resource: "GP entry",
        limit: entry_limit,
    })?;
    file.take(read_limit).read_to_end(&mut data)?;
    if data.len() > entry_limit {
        return Err(Error::LimitExceeded {
            resource: "GP entry",
            limit: entry_limit,
        });
    }
    let compressed = usize::try_from(file.compressed_size()).map_err(|_| Error::LimitExceeded {
        resource: "GP compressed bytes",
        limit: usize::MAX,
    })?;
    if data.len() > compressed.saturating_mul(limits.max_compression_ratio) {
        return Err(Error::LimitExceeded {
            resource: "GP compression ratio",
            limit: limits.max_compression_ratio,
        });
    }
    *total = total.checked_add(data.len()).ok_or(Error::LimitExceeded {
        resource: "GP total uncompressed bytes",
        limit: limits.max_total_uncompressed_bytes,
    })?;
    if *total > limits.max_total_uncompressed_bytes {
        return Err(Error::LimitExceeded {
            resource: "GP total uncompressed bytes",
            limit: limits.max_total_uncompressed_bytes,
        });
    }
    Ok(data)
}

/// Extracts the GPIF payload from GP7/8 container bytes.
pub(crate) fn read_payload(bytes: &[u8]) -> Result<String> {
    read_payload_with_limits(bytes, DEFAULT_CONTAINER_LIMITS)
}

/// Extracts a GPIF payload using caller-supplied container limits.
pub(crate) fn read_payload_with_limits(bytes: &[u8], limits: ContainerLimits) -> Result<String> {
    validate_limits(limits)?;
    if bytes.len() > limits.max_archive_bytes {
        return Err(Error::LimitExceeded {
            resource: "GP archive bytes",
            limit: limits.max_archive_bytes,
        });
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(wrap)?;
    if zip.len() > limits.max_entries {
        return Err(Error::LimitExceeded {
            resource: "GP archive entries",
            limit: limits.max_entries,
        });
    }
    let mut total = 0;
    let mut payload = None;
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).map_err(wrap)?;
        let name = file.name().to_string();
        let data = read_entry(&mut file, limits, &mut total)?;
        if name.ends_with("score.gpif") {
            payload = Some(String::from_utf8(data).map_err(|e| Error::format("gp", e))?);
        }
    }
    payload.ok_or_else(|| Error::Malformed("no score payload in the archive".into()))
}

/// Reads a score payload and retained entries using caller-supplied limits.
pub(crate) fn read_source_with_limits(
    bytes: &[u8],
    limits: ContainerLimits,
) -> Result<(String, RetainedEntries)> {
    validate_limits(limits)?;
    if bytes.len() > limits.max_archive_bytes {
        return Err(Error::LimitExceeded {
            resource: "GP archive bytes",
            limit: limits.max_archive_bytes,
        });
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(wrap)?;
    if zip.len() > limits.max_entries {
        return Err(Error::LimitExceeded {
            resource: "GP archive entries",
            limit: limits.max_entries,
        });
    }
    let mut payload = None;
    let mut entries = Vec::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(wrap)?;
        let name = file.name().to_string();
        let data = read_entry(&mut file, limits, &mut total)?;
        if name.ends_with("score.gpif") {
            payload = Some(String::from_utf8(data).map_err(|e| Error::format("gp", e))?);
        } else {
            entries.push((name, data));
        }
    }
    payload
        .map(|payload| (payload, entries))
        .ok_or_else(|| Error::Malformed("no score payload in the archive".into()))
}

/// Reads every entry in archive order, directories included (empty data).
pub(crate) fn read_entries(bytes: &[u8]) -> Result<RetainedEntries> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(wrap)?;
    let mut entries = Vec::new();
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(wrap)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        entries.push((file.name().to_string(), data));
    }
    Ok(entries)
}

/// Writes entries in the given order with the same fixed timestamp.
pub(crate) fn write_entries(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::from_date_and_time(2020, 1, 1, 0, 0, 0).unwrap());
        for (name, data) in entries {
            if name.ends_with('/') {
                zip.add_directory(name, opts).map_err(wrap)?;
            } else {
                zip.start_file(name, opts).map_err(wrap)?;
                zip.write_all(data)?;
            }
        }
        zip.finish().map_err(wrap)?;
    }
    Ok(buf)
}

/// Writes a container with a fixed timestamp so output is byte-deterministic --
/// the zip writer stamps wall-clock time otherwise, which has broken
/// reproducibility in this project before.
pub(crate) fn write_payload(xml: &str) -> Result<Vec<u8>> {
    write_payload_with_entries(xml, &[])
}

/// Writes a score payload together with retained unmodelled entries.
pub(crate) fn write_payload_with_entries(
    xml: &str,
    entries: &[(String, Vec<u8>)],
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::from_date_and_time(2020, 1, 1, 0, 0, 0).unwrap());
        zip.start_file(GPIF_PATH, opts).map_err(wrap)?;
        zip.write_all(xml.as_bytes())?;
        for (name, data) in entries {
            zip.start_file(name, opts).map_err(wrap)?;
            zip.write_all(data)?;
        }
        zip.finish().map_err(wrap)?;
    }
    Ok(buf)
}

#[cfg(test)]
#[path = "container_test.rs"]
mod tests;
