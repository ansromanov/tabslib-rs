use super::*;

#[test]
fn oversized_entry_is_rejected_without_reading_it_fully() {
    let bytes =
        write_payload_with_entries("<GPIF/>", &[("extra".to_string(), vec![b'x'; 32])]).unwrap();
    let limits = ContainerLimits {
        max_archive_bytes: bytes.len(),
        max_entries: 10,
        max_entry_bytes: 8,
        max_total_uncompressed_bytes: 100,
        max_compression_ratio: 1000,
    };
    let error = read_source_with_limits(&bytes, limits).unwrap_err();
    assert!(matches!(
        error,
        Error::LimitExceeded {
            resource: "GP entry",
            ..
        }
    ));
}

#[test]
fn entry_count_is_bounded() {
    let entries = (0..4)
        .map(|index| (format!("extra-{index}"), vec![1]))
        .collect::<Vec<_>>();
    let bytes = write_payload_with_entries("<GPIF/>", &entries).unwrap();
    let limits = ContainerLimits {
        max_archive_bytes: bytes.len(),
        max_entries: 2,
        max_entry_bytes: 1024,
        max_total_uncompressed_bytes: 4096,
        max_compression_ratio: 1000,
    };
    let error = read_source_with_limits(&bytes, limits).unwrap_err();
    assert!(matches!(
        error,
        Error::LimitExceeded {
            resource: "GP archive entries",
            ..
        }
    ));
}
