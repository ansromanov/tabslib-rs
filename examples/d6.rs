use tabslib::{format::gp::Gp, pitch, ReadFormat};
fn main() {
    let dir = std::env::var("TABSLIB_DBT").unwrap() + "/corpus/source/songs";
    let mut d = Gp::read(
        &std::fs::read(format!("{dir}/Damned By Time - 05 - Cursed Way (C#).gp")).unwrap(),
    )
    .unwrap();
    println!("{:?}", pitch::transpose(&mut d, 2));
}
