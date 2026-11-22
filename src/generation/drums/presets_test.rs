use super::*;

#[test]
fn presets_and_velocity_layers_are_deterministic() {
    let mut hits = vec![DrumHit::new(36, 0), DrumHit::new(38, 1)];
    hits[1].ghost = true;
    shape_velocities(&mut hits, 100);
    assert!(hits[0].velocity > hits[1].velocity);
    apply_variations(&mut hits, 5);
    assert_eq!(resolve_auto_preset("rock"), Some(rock_straight_preset()));
}
