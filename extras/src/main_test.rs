use super::*;

#[test]
fn output_refuses_an_existing_path() {
    let path = std::env::temp_dir().join(format!("tabs-test-{}", std::process::id()));
    fs::write(&path, b"existing").expect("create test file");
    assert!(refuse_overwrite(path.to_str().expect("utf-8 path")).is_err());
    fs::remove_file(path).expect("remove test file");
}

#[test]
fn one_based_number_parser_reports_missing_values() {
    let value = "3".to_string();
    assert_eq!(number(Some(&value), "bar").expect("number"), 3);
    assert!(number(None, "bar").is_err());
}
