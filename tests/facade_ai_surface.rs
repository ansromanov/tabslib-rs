#![cfg(feature = "ai")]

use tabslib::facade::generation::{
    capability_status, generate_section_boundaries, generate_section_labels, AiFailureReason,
    Capability, CapabilityStatus, CapturedRequest, RationaleProvider, SectionFingerprint,
};

struct MockProvider(&'static str);

impl RationaleProvider for MockProvider {
    fn name(&self) -> &str {
        "facade-mock"
    }

    fn complete(&self, _: &CapturedRequest) -> Result<String, String> {
        Ok(self.0.into())
    }
}

#[test]
fn ai_section_prompts_are_exposed_through_the_facade() {
    let sections = [
        SectionFingerprint {
            id: "caller-a".into(),
            features: std::collections::BTreeMap::new(),
        },
        SectionFingerprint {
            id: "caller-b".into(),
            features: std::collections::BTreeMap::new(),
        },
    ];
    assert_eq!(
        capability_status(Capability::AiSectionPrompts),
        CapabilityStatus::Available
    );
    assert_eq!(
        tabslib::facade::generation::classify_provider_failure("auth: HTTP 401"),
        AiFailureReason::Auth
    );
    let labels = MockProvider(
        r#"{"labels":[{"section_index":0,"label":"a"},{"section_index":1,"label":"b"}]}"#,
    );
    assert_eq!(
        generate_section_labels(&labels, &sections)
            .unwrap()
            .labels
            .len(),
        2
    );
    let boundaries = MockProvider(r#"{"merges":[],"splits_after":[0]}"#);
    assert_eq!(
        generate_section_boundaries(&boundaries, &sections)
            .unwrap()
            .splits_after,
        [0]
    );
}
