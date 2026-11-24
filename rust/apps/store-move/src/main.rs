#![forbid(unsafe_code)]
//! `vimbai-store-move <from> <to> [--plan] [--receipt <path>] [--dump <path>] [--into-occupied] [--rehearse]`
//!
//! Exits 1 if the destination does not hold what the source held, because a move that cannot be trusted should not be
//! scripted past.

use std::path::PathBuf;
use std::process::ExitCode;

use vimbai_store_move::{cutover, move_guarded};

const USAGE: &str = "\
vimbai-store-move - move a store's records into another store

USAGE:
    vimbai-store-move <from> <to> [options]

    <from>, <to>       file:<path> | sqlite:<path> | postgres:<dsn> | neo4j

OPTIONS:
    --plan             say what the move would carry, and whether the destination is empty, then stop
    --receipt <path>   write a receipt of what happened to this file
    --dump <path>      write the dump to this path as well, so the move can be replayed
    --into-occupied    allow a destination that already holds records (it will mix two books)
    --rehearse         mark the receipt as a rehearsal, so nobody reads it as a cutover

Neo4j is configured by NEO4J_URI, NEO4J_USER, NEO4J_PASSWORD and NEO4J_DATABASE.
Nothing is removed from the source: to retire a database, move the data, run the engine against the new one, and keep the
old one until the new one has served a week.
";

struct Asked {
    from: String,
    to: String,
    plan_only: bool,
    receipt: Option<PathBuf>,
    dump: Option<PathBuf>,
    into_occupied: bool,
    rehearsed: bool,
}

fn asked(args: &[String]) -> Option<Asked> {
    let mut positional = Vec::new();
    let mut plan_only = false;
    let mut receipt = None;
    let mut dump = None;
    let mut into_occupied = false;
    let mut rehearsed = false;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--plan" => plan_only = true,
            "--into-occupied" => into_occupied = true,
            "--rehearse" => rehearsed = true,
            "--receipt" => receipt = rest.next().map(PathBuf::from),
            "--dump" => dump = rest.next().map(PathBuf::from),
            other if other.starts_with("--") => return None,
            other => positional.push(other.to_string()),
        }
    }
    match positional.as_slice() {
        [from, to, ..] => Some(Asked {
            from: from.clone(),
            to: to.clone(),
            plan_only,
            receipt,
            dump,
            into_occupied,
            rehearsed,
        }),
        _ => None,
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(asked) = asked(&args) else {
        print!("{USAGE}");
        return ExitCode::from(2);
    };
    if asked.plan_only {
        return match cutover(&asked.from, &asked.to, usize::MAX / 2) {
            Ok(ahead) => {
                print!("{}", ahead.to_text());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("the plan could not be worked out: {error}");
                ExitCode::FAILURE
            }
        };
    }
    match move_guarded(
        &asked.from,
        &asked.to,
        usize::MAX / 2,
        asked.dump.as_deref(),
        asked.into_occupied,
        asked.rehearsed,
    ) {
        Ok(receipt) => {
            if let Some(path) = &asked.receipt {
                if let Err(error) = receipt.write(path) {
                    eprintln!("the move happened but the receipt could not be written: {error}");
                    return ExitCode::FAILURE;
                }
            }
            print!("{}", receipt.to_text());
            if receipt.faithful() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("the move did not happen: {error}");
            ExitCode::FAILURE
        }
    }
}
