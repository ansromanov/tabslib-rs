use super::*;

#[test]
fn deep_xml_is_rejected_by_the_depth_limit() {
    let xml = "<GPIF><A><B><C/></B></A></GPIF>";
    let error = parse_xml_with_limits(
        xml,
        XmlLimits {
            max_xml_bytes: 1024,
            max_depth: 2,
            max_nodes: 100,
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        crate::Error::LimitExceeded {
            resource: "GPIF XML depth",
            ..
        }
    ));
}

#[test]
fn xml_node_count_is_bounded() {
    let error = parse_xml_with_limits(
        "<GPIF><A/><B/></GPIF>",
        XmlLimits {
            max_xml_bytes: 1024,
            max_depth: 10,
            max_nodes: 2,
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        crate::Error::LimitExceeded {
            resource: "GPIF XML nodes",
            ..
        }
    ));
}
