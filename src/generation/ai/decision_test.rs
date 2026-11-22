use super::*;
use serde_json::json;

#[test]
fn cache_prevents_repeated_provider_calls_and_falls_back() {
    let mut store = MemoryDecisionStore::default();
    let request = DecisionRequest {
        namespace: "test".into(),
        input: json!({"bar": 2}),
    };
    let first = resolve_decision(
        &mut store,
        &request,
        |_| Err("offline".into()),
        |_| json!("safe"),
    )
    .expect("fallback should resolve");
    assert_eq!(first.source, DecisionSource::Fallback);
    let second = resolve_decision(
        &mut store,
        &request,
        |_| Ok(json!("provider")),
        |_| json!("other"),
    )
    .expect("cache should resolve");
    assert_eq!(second.source, DecisionSource::Cache);
    assert_eq!(second.value, json!("safe"));
}

#[test]
fn interactive_and_named_decisions_replay_from_the_same_cache() {
    let mut store = MemoryDecisionStore::default();
    let mut calls = 0;
    let mut interactive = InteractiveDecisionProvider::new(|request: &DecisionRequest| {
        assert_eq!(request.namespace, "section-decisions");
        Ok(serde_json::json!({ "decisions": ["keep"] }))
    });
    let first = resolve_section_decisions(
        &mut store,
        serde_json::json!({ "sections": 1 }),
        |request| {
            calls += 1;
            interactive.decide(request)
        },
    )
    .unwrap();
    let second =
        resolve_section_decisions(&mut store, serde_json::json!({ "sections": 1 }), |_| {
            panic!("cached decision must not call provider")
        })
        .unwrap();
    assert_eq!(first.value, second.value);
    assert_eq!(calls, 1);
    assert_eq!(section_decisions_action_space(2).len(), 2);
    assert_eq!(boundary_decisions_action_space(2).len(), 1);
    assert_eq!(rationale_prose_action_space().len(), 1);
}
