use tabslib::facade::generation::{
    capability_status, preset_name_for_tuning, resolve_tuning_preset, retune_preserve_fingering,
    retune_preserve_pitch, Capability, CapabilityStatus, PitchError, PitchTarget, TuningPreset,
    TuningPresetError, TUNING_PRESETS,
};

#[test]
fn named_tunings_and_name_aware_retuning_are_available_through_the_facade() {
    assert_eq!(
        capability_status(Capability::NamedTuningPresets),
        CapabilityStatus::Available
    );
    let drop_d: &TuningPreset = resolve_tuning_preset("drop d").unwrap();
    assert_eq!(preset_name_for_tuning(drop_d.pitches), Some("Drop D"));
    assert_eq!(TUNING_PRESETS.len(), 14);
    let _invalid_name: Option<TuningPresetError> = None;
    let _pitch_error: Option<PitchError> = None;
    let _target = PitchTarget::Preset("E Standard");

    let original = tabslib::fixtures::repeated_frets();
    let mut pitch_kept = original.clone();
    let preset_name = String::from("Eb Standard");
    retune_preserve_pitch(&mut pitch_kept, 0, &preset_name).unwrap();
    assert_eq!(pitch_kept.tracks[0].tuning[0], 39);
    let mut fingering_kept = original;
    retune_preserve_fingering(&mut fingering_kept, 0, "Drop D").unwrap();
    assert_eq!(fingering_kept.tracks[0].tuning[0], 38);
}
