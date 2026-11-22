use super::*;

#[test]
fn named_presets_and_spans_are_stable() {
    assert_eq!(pentatonic_scale_steps(), &[0, 2, 4, 7, 9]);
    assert_eq!(calculate_lead_beat_dynamics(50), 63);
    assert_eq!(
        plan_solo_spans(&[2, 0, 3]),
        vec![
            LeadSpan {
                start_bar: 0,
                end_bar: 1
            },
            LeadSpan {
                start_bar: 2,
                end_bar: 4
            }
        ]
    );
}
