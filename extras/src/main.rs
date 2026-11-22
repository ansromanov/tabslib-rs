//! Small, deterministic command-line chores for tablature files.

use std::env;
use std::fmt::Debug;
use std::fs;
use std::path::Path;

use tabslib::format::ascii::{render_compare, render_track};
use tabslib::format::gp::Gp;
use tabslib::format::midi;
use tabslib::format::WriteFormat;
use tabslib::{edits, load, pitch};

fn debug_error<E: Debug>(error: E) -> String {
    format!("{error:?}")
}

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        Some("info") => info(&args[1..]),
        Some("tab") => tab(&args[1..]),
        Some("diff") => diff(&args[1..]),
        Some("transpose") => transpose(&args[1..]),
        Some("retune") => retune(&args[1..]),
        Some("slice") => slice(&args[1..]),
        Some("append") => append(&args[1..]),
        Some("section") => section(&args[1..]),
        Some("silence") => silence(&args[1..]),
        Some("drop-track") => drop_track(&args[1..]),
        Some("midi") => midi_export(&args[1..]),
        _ => Err("usage: tabs <info|tab|diff|transpose|retune|slice|append|section|silence|drop-track|midi> ...".into()),
    }
}

fn score(path: &str) -> Result<tabslib::Document, Box<dyn std::error::Error>> {
    Ok(load(&fs::read(path)?)?)
}

fn output(path: &str, document: &tabslib::Document) -> Result<(), Box<dyn std::error::Error>> {
    refuse_overwrite(path)?;
    fs::write(path, <Gp as WriteFormat>::write(document)?)?;
    Ok(())
}

fn refuse_overwrite(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    if Path::new(path).exists() {
        return Err(format!("refusing to overwrite {path}").into());
    }
    Ok(())
}

fn number(value: Option<&String>, name: &str) -> Result<usize, Box<dyn std::error::Error>> {
    value
        .ok_or_else(|| format!("missing {name}").into())
        .and_then(|value| Ok(value.parse()?))
}

fn info(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 1 {
        return Err("usage: tabs info <file.gp>".into());
    }
    let document = score(&args[0])?;
    println!("title: {}", document.title);
    println!("artist: {}", document.artist);
    println!("tracks: {}", document.tracks.len());
    println!("bars: {}", document.master_bars.len());
    println!("notes: {}", document.note_count());
    Ok(())
}

fn tab(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.is_empty() {
        return Err("usage: tabs tab <file.gp> [--track N] [--start N] [--end N]".into());
    }
    let mut track = 1;
    let mut start = 1;
    let mut end = None;
    let mut index = 1;
    while index < args.len() {
        let value = number(args.get(index + 1), &args[index])?;
        match args[index].as_str() {
            "--track" => track = value,
            "--start" => start = value,
            "--end" => end = Some(value),
            flag => return Err(format!("unknown option {flag}").into()),
        }
        index += 2;
    }
    let document = score(&args[0])?;
    let end = end.unwrap_or(document.master_bars.len());
    println!(
        "{}",
        render_track(&document, track - 1, start - 1, end - 1)?
    );
    Ok(())
}

fn diff(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 2 {
        return Err("usage: tabs diff <left.gp> <right.gp>".into());
    }
    let left = score(&args[0])?;
    let right = score(&args[1])?;
    println!("{}", render_compare(&left, 0, (0, 0), (0, 0))?);
    println!("--- right ---");
    println!("{}", render_track(&right, 0, 0, 0)?);
    Ok(())
}

fn transform<F>(args: &[String], operation: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnOnce(&mut tabslib::Document) -> Result<(), Box<dyn std::error::Error>>,
{
    if args.len() != 3 {
        return Err("usage: tabs <command> <input.gp> <output.gp> <value>".into());
    }
    let mut document = score(&args[0])?;
    operation(&mut document)?;
    output(&args[1], &document)
}

fn transpose(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    transform(args, |document| {
        pitch::transpose(document, args[2].parse()?).map_err(debug_error)?;
        Ok(())
    })
}

fn retune(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    transform(args, |document| {
        let tuning = args[2]
            .split(',')
            .map(str::parse)
            .collect::<Result<Vec<i32>, _>>()?;
        pitch::retune_preserve_pitch(document, 0, &tuning).map_err(debug_error)?;
        Ok(())
    })
}

fn slice(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err("usage: tabs slice <input.gp> <output.gp> <start> <end>".into());
    }
    let document = score(&args[0])?;
    let sliced = edits::slice(
        &document,
        number(args.get(2), "start")? - 1,
        number(args.get(3), "end")? - 1,
    )
    .map_err(debug_error)?;
    output(&args[1], &sliced)
}

fn append(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: tabs append <destination.gp> <source.gp> <output.gp>".into());
    }
    let mut destination = score(&args[0])?;
    edits::append(&mut destination, &score(&args[1])?).map_err(debug_error)?;
    output(&args[2], &destination)
}

fn section(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err("usage: tabs section <input.gp> <output.gp> <bar> <label>".into());
    }
    let mut document = score(&args[0])?;
    edits::set_section(
        &mut document,
        args[2].parse::<usize>()? - 1,
        Some(args[3].clone()),
    )
    .map_err(debug_error)?;
    output(&args[1], &document)
}

fn silence(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 4 {
        return Err("usage: tabs silence <input.gp> <output.gp> <start> <end>".into());
    }
    let mut document = score(&args[0])?;
    edits::silence(
        &mut document,
        args[2].parse::<usize>()? - 1,
        args[3].parse::<usize>()? - 1,
    )
    .map_err(debug_error)?;
    output(&args[1], &document)
}

fn drop_track(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 3 {
        return Err("usage: tabs drop-track <input.gp> <output.gp> <track>".into());
    }
    let mut document = score(&args[0])?;
    edits::remove_track(&mut document, args[2].parse::<usize>()? - 1).map_err(debug_error)?;
    output(&args[1], &document)
}

fn midi_export(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() != 2 {
        return Err("usage: tabs midi <input.gp> <output.mid>".into());
    }
    refuse_overwrite(&args[1])?;
    fs::write(&args[1], midi::write(&score(&args[0])?))?;
    Ok(())
}

#[cfg(test)]
#[path = "main_test.rs"]
mod tests;
