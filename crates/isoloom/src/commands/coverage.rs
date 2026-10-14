//! `isoloom coverage`: which features of each output format (Compose, Vagrant, Terraform) a spec
//! can produce, and which fields of a spec each output handles (the spec table), as a summary,
//! every row, Markdown, or JSON for the website.

use std::process::ExitCode;

use isoloom_coverage::spec::{Output, score, table};
use isoloom_coverage::{Format, Support, anchor, capitalize, formats, markdown, section};

use crate::{Res, output};

#[derive(clap::Args)]
pub struct Args {
    /// Every feature, with how Isoloom produces it or why not.
    #[arg(long)]
    all: bool,
    /// The full coverage page as Markdown (both directions).
    #[arg(long)]
    markdown: bool,
    /// Every format, its score and every feature, as JSON (for the website).
    #[arg(long)]
    json: bool,
}

pub fn run(a: &Args) -> Res<ExitCode> {
    if a.markdown {
        print!("{}", markdown());
        return Ok(ExitCode::SUCCESS);
    }
    if a.json {
        let out: Vec<serde_json::Value> = formats().iter().map(format_json).collect();
        output::print_json(&out)?;
        return Ok(ExitCode::SUCCESS);
    }
    for f in formats() {
        println!("{}: {}", f.name, f.summary());
        if a.all {
            let width = f.rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
            for (key, s) in &f.rows {
                println!("  {key:width$}  {:<6}  {}", s.implemented(), s.note());
            }
            println!();
        }
    }
    spec_table(a.all);
    if !a.all {
        println!("\nEvery feature, with how or why not: isoloom coverage --all · as Markdown: --markdown");
    }
    Ok(ExitCode::SUCCESS)
}

/// The spec's side: for each output, how many of the spec's fields it handles, and with
/// `all`, each field it doesn't handle yet and why.
fn spec_table(all: bool) {
    let rows = table();
    println!("\nSpec fields handled, per output:");
    for o in Output::all() {
        let s = score(&rows, o);
        println!(
            "  {:<14} {:>3}% of {} ({} done, {} partly, {} planned, {} refused)",
            o.label(),
            s.percent(),
            s.applicable(),
            s.done,
            s.partial,
            s.planned,
            s.refused
        );
        if all {
            for (path, cell) in rows.iter().filter_map(|r| r.cell(o).map(|c| (r.path, c))) {
                if matches!(cell.symbol(), "◐" | "planned" | "✗") {
                    println!("      {path}: {}", cell.note());
                }
            }
        }
    }
}

/// One format for the website: its score and its rows by section.
fn format_json(f: &Format) -> serde_json::Value {
    let score = f.score();
    let mut sections: Vec<serde_json::Value> = Vec::new();
    let mut collapsed = 0;
    for (key, s) in &f.rows {
        if f.collapse_not_portable && matches!(s, Support::NotPortable { .. }) {
            collapsed += 1;
            continue;
        }
        let (title, name) = section(key);
        if sections.last().and_then(|x| x["title"].as_str()) != Some(title) {
            sections.push(serde_json::json!({ "title": title, "rows": [] }));
        }
        let row = serde_json::json!({
            "key": name,
            "every_target": s.portable(),
            "implemented": s.implemented(),
            "note": capitalize(&s.note()),
        });
        if let Some(rows) = sections.last_mut().and_then(|x| x["rows"].as_array_mut()) {
            rows.push(row);
        }
    }
    serde_json::json!({
        "name": f.name,
        "anchor": anchor(f.name),
        "source": f.source,
        "features": f.rows.len(),
        "score": { "percent": score.percent(), "portable": score.portable, "done": score.done, "partly": score.partly, "to_do": score.to_do },
        "collapsed_not_portable": collapsed,
        "sections": sections,
    })
}
