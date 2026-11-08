//! Batch presentation edit for `.gp` files: fader levels, 150% zoom, tablature
//! only, and a tuning label appended to the file name.
//!
//! `cargo run --example restyle -- <dir> [--apply]`
//!
//! Without `--apply` it only prints the plan. With it, each file is rewritten
//! through a temporary file and renamed, so an interrupted run never leaves a
//! half-written score. Files already named `... [<tuning>].gp` are not renamed
//! twice, and a rename that would overwrite another file is refused.

use std::path::{Path, PathBuf};

use tabslib::format::gp::{restyle, Gp, Restyle, VolumeDb};
use tabslib::{tuning, ReadFormat};

const VOLUMES: VolumeDb = VolumeDb {
    guitar: -2.0,
    bass: -1.0,
    drums: 0.0,
    other: -3.0,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("usage: restyle <dir> [--apply]");
        std::process::exit(2);
    };
    let apply = args.any(|a| a == "--apply");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("read directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("gp")))
        .collect();
    files.sort();
    let opts = Restyle {
        zoom: Some(1.5),
        tab_only: true,
        volumes: Some(VOLUMES),
    };
    let mut failed = 0;
    for path in &files {
        match process(path, &opts, apply) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                failed += 1;
                println!("FAIL {}: {e}", path.display());
            }
        }
    }
    println!("{} file(s), {failed} failed, apply={apply}", files.len());
    if failed > 0 {
        std::process::exit(1);
    }
}

fn process(path: &Path, opts: &Restyle, apply: bool) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let label = Gp::read(&bytes)
        .map_err(|e| e.to_string())
        .and_then(|doc| tuning::score_label(&doc).ok_or_else(|| "tuning not recognised".into()))?;
    let out = restyle(&bytes, opts).map_err(|e| e.to_string())?;
    let target = renamed(path, &label)?;
    if apply {
        let tmp = path.with_extension("gp.tmp");
        std::fs::write(&tmp, &out).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
        if target != path {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    Ok(format!("{:?} -> {:?}", name(path), name(&target)))
}

/// `Song.gp` becomes `Song [Drop D].gp`; a name that already carries a bracketed
/// tuning label is left alone.
fn renamed(path: &Path, label: &str) -> Result<PathBuf, String> {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    let stem = stem.ok_or("no file name")?;
    let suffix = format!(" [{label}]");
    if stem.ends_with(&suffix) {
        return Ok(path.to_path_buf());
    }
    let target = path.with_file_name(format!("{stem}{suffix}.gp"));
    if target.exists() {
        return Err(format!("{} already exists", target.display()));
    }
    Ok(target)
}
