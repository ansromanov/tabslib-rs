//! Every public error type must work with `?` in a `Box<dyn Error>` context.
//! Each check fails to compile if a type loses its `std::error::Error` impl.

use tabslib::edits::EditError;
use tabslib::pitch::PitchError;
use tabslib::selection::SelectionError;

fn assert_error<E: std::error::Error + Send + Sync + 'static>() {}

#[test]
fn public_error_types_implement_std_error() {
    assert_error::<tabslib::Error>();
    assert_error::<EditError>();
    assert_error::<PitchError>();
    assert_error::<SelectionError>();
}

#[test]
fn pitch_error_converts_through_question_mark() {
    fn transpose_out_of_range() -> Result<(), Box<dyn std::error::Error>> {
        let mut doc = tabslib::fixtures::power_chords();
        let bars = doc.master_bars.len();
        tabslib::pitch::transpose_track(&mut doc, 0, bars, bars, 2)?;
        Ok(())
    }
    let message = transpose_out_of_range().unwrap_err().to_string();
    assert!(message.starts_with("invalid selection"), "{message}");
}
