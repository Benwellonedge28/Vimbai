//! Repository tooling.
//!
//! What lives here is the work the Python scripts did that must outlive them: the generators that keep a derived file
//! honest, and the gates that fail when the estate and its description disagree. It is a binary rather than a crate of
//! helpers because each of those jobs is something a person runs and reads the output of, and because a test can call
//! the same functions and assert the answer.
#![forbid(unsafe_code)]

use std::process::ExitCode;

use xtask::catalog;

const USAGE: &str = "\
xtask - repository tooling

USAGE:
    cargo run -p xtask -- <command>

COMMANDS:
    catalog            rewrite catalog/ from the gateway's routes and catalog/ownership.json
    catalog --check    report whether the committed catalogue is current, and exit 1 if it is not
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("");
    let root = catalog::repo_root();

    match command {
        "catalog" => {
            let check = args.iter().any(|argument| argument == "--check");
            match catalog::write(&root, check) {
                Ok(()) => {
                    if check {
                        println!("catalog: current");
                    } else {
                        let rows = catalog::rows(&root);
                        let domains = catalog::domain_texts(&root).len();
                        println!(
                            "wrote catalog: {} services across {} domains",
                            rows.len(),
                            domains
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(problem) => {
                    eprintln!("{problem}");
                    ExitCode::FAILURE
                }
            }
        }
        "" | "help" | "--help" | "-h" => {
            print!("{USAGE}");
            if command.is_empty() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        other => {
            eprintln!("xtask: no command called {other}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
