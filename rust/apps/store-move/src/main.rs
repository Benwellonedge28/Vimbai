#![forbid(unsafe_code)]
//! `vimbai-store-move <from> <to> [dump file]`
//!
//! Exits 1 if the destination does not hold what the source held, because a move that cannot be trusted should not be
//! scripted past.

use std::process::ExitCode;

use vimbai_store_move::move_all;

const USAGE: &str = "\
vimbai-store-move - move a store's records into another store

USAGE:
    vimbai-store-move <from> <to> [dump file]

    <from>, <to>     file:<path>  or  neo4j
    [dump file]      write the dump to this path as well, so the move can be replayed

Neo4j is configured by NEO4J_URI, NEO4J_USER, NEO4J_PASSWORD and NEO4J_DATABASE.
Nothing is removed from the source: to retire a database, move the data, run the engine against the new one, and keep the
old one until the new one has served a week.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        print!("{USAGE}");
        return ExitCode::from(2);
    }
    let dump_path = args.get(2).map(std::path::PathBuf::from);
    match move_all(&args[0], &args[1], usize::MAX / 2, dump_path.as_deref()) {
        Ok((restored, differences)) if differences.is_empty() => {
            println!(
                "moved {} records and {} edges; the destination holds what the source held",
                restored.records, restored.edges
            );
            ExitCode::SUCCESS
        }
        Ok((restored, differences)) => {
            println!(
                "moved {} records and {} edges, and {} things do not match",
                restored.records,
                restored.edges,
                differences.len()
            );
            for line in differences.iter().take(50) {
                println!("  {line}");
            }
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("the move did not happen: {error}");
            ExitCode::FAILURE
        }
    }
}
