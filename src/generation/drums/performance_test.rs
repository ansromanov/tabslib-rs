use super::*;
use crate::generation::drums::voices;

#[test]
fn performance_layer_empty() {
    let ctx = PerformanceLayerConfig::default();
    let hits = apply_performance_layer(&[], &ctx);
    assert!(hits.is_empty());
}

#[test]
fn performance_layer_ghost_and_accent_gating() {
    let mut hit1 = DrumHit::new(voices::ACOUSTIC_SNARE, 4);
    hit1.ghost = true;
    let mut hit2 = DrumHit::new(voices::CLOSED_HI_HAT, 0);
    hit2.accent = true;

    let mut ctx = PerformanceLayerConfig::new(120);
    ctx.ghost_notes = false;
    ctx.accents = false;

    let result = apply_performance_layer(&[hit1, hit2], &ctx);
    // hit1 (ghost) should be removed
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].midi, voices::CLOSED_HI_HAT);
    // hit2 accent should be gated off
    assert!(!result[0].accent);
}

#[test]
fn performance_layer_stab_catches_and_backbeat_unghost() {
    let mut snare_ghost = DrumHit::new(voices::ACOUSTIC_SNARE, 4);
    snare_ghost.ghost = true;

    let mut ctx = PerformanceLayerConfig::new(140);
    ctx.intensity = 2; // Hot intensity unghosts backbeat snare
    ctx.backbeat_steps = vec![4];
    ctx.stab_steps = vec![8]; // Adds kick + china at step 8

    let result = apply_performance_layer(&[snare_ghost], &ctx);

    // Snare at step 4 should now be unghosted and accented
    let snare = result
        .iter()
        .find(|h| h.midi == voices::ACOUSTIC_SNARE && h.step == 4)
        .expect("snare present");
    assert!(!snare.ghost);
    assert!(snare.accent);

    // Stab catch at step 8 should have kick and china
    let kick = result
        .iter()
        .find(|h| h.midi == voices::ACOUSTIC_BASS_DRUM && h.step == 8);
    let china = result
        .iter()
        .find(|h| h.midi == voices::CHINESE_CYMBAL && h.step == 8);
    assert!(kick.is_some());
    assert!(china.is_some());
}

#[test]
fn performance_layer_hihat_choke_pairing() {
    let open_hat = DrumHit::new(voices::OPEN_HI_HAT, 0);
    let closed_hat = DrumHit::new(voices::CLOSED_HI_HAT, 4);

    let ctx = PerformanceLayerConfig::new(120);
    let result = apply_performance_layer(&[open_hat, closed_hat], &ctx);

    let open = result
        .iter()
        .find(|h| h.midi == voices::OPEN_HI_HAT)
        .expect("open hat present");
    assert_eq!(open.choked_at_step, Some(4));
}
