//! Load / save / walk timing, comparable to the same operation in the
//! TypeScript engine.
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("usage: bench <file.gp>");
    let bytes = std::fs::read(&path).expect("read");

    let t = Instant::now();
    let doc = tabslib::load(&bytes).expect("load");
    let load_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let out = tabslib::save(&doc).expect("save");
    let save_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let mut n = 0usize;
    for _ in 0..200 {
        n += doc.note_count();
    }
    let walk_ms = t.elapsed().as_secs_f64() * 1000.0;

    println!(
        "  load {:>6.1} ms   save {:>6.1} ms   walk200 {:>5.1} ms   notes {}   out {} bytes",
        load_ms,
        save_ms,
        walk_ms,
        n / 200,
        out.len()
    );
}
