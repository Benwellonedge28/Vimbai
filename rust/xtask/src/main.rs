//! Repository tooling.
//!
//! What lives here is the work the Python scripts did that must outlive them: the generators that keep a derived file
//! honest, and the gates that fail when the estate and its description disagree. It is a binary rather than a crate of
//! helpers because each of those jobs is something a person runs and reads the output of, and because a test can call
//! the same functions and assert the answer.
#![forbid(unsafe_code)]

use std::process::ExitCode;

use xtask::{boundaries, catalog, gateway, reachability};

const USAGE: &str = "\
xtask - repository tooling

USAGE:
    cargo run -p xtask -- <command>

COMMANDS:
    catalog            rewrite catalog/ from the gateway's routes and catalog/ownership.json
    catalog --check    report whether the committed catalogue is current, and exit 1 if it is not
    gateway            report the addresses the gateway publishes that the engine does not answer
    gateway --addresses  print every address the engine answers, one per line
    reachability       report the modules no door can reach, one per line
    boundaries         report crate dependency rule violations, and exit 1 if there are any
    boundaries --graph print every crate with its family and its internal dependencies
    boundaries --rules print the rule table as the documentation states it
    boundaries --exceptions print the tolerated dependencies and why
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
        // The modules nothing can call: arithmetic that looks wired because it is `pub` and re-exported.
        "reachability" => {
            let dead = reachability::unreachable_modules(&root);
            for module in &dead {
                println!("{module}");
            }
            if dead.is_empty() {
                println!("reachability: every module has a door");
            }
            ExitCode::SUCCESS
        }
        "gateway" if args.iter().any(|argument| argument == "--addresses") => {
            for path in gateway::engine_routes(&root) {
                println!("{path}");
            }
            ExitCode::SUCCESS
        }
        "gateway" => {
            let mut problems = 0;
            let routes = gateway::engine_routes(&root).len();
            println!("engine addresses: {routes}");

            let addresses = gateway::audit(&root);
            println!(
                "addresses a retired service publishes that nothing answers: {}",
                addresses.len()
            );
            for line in &addresses {
                println!("  {line}");
            }
            problems += addresses.len();

            let methods = gateway::method_gaps(&root);
            println!("published methods nothing answers: {}", methods.len());
            for line in &methods {
                println!("  {line}");
            }
            problems += methods.len();

            let collisions = gateway::stripped_collisions(&root);
            println!("stripped paths two services claim: {}", collisions.len());
            for (path, names) in &collisions {
                println!("  {path}: {}", names.join(", "));
            }
            problems += collisions.len();

            if problems == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
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
        "boundaries" if args.iter().any(|argument| argument == "--rules") => {
            for rule in boundaries::rules() {
                println!("{} [{}] {}", rule.id, rule.from.label(), rule.statement);
            }
            ExitCode::SUCCESS
        }
        "boundaries" if args.iter().any(|argument| argument == "--exceptions") => {
            for exception in boundaries::exceptions() {
                println!(
                    "{} {} -> {}\n    {}",
                    exception.rule, exception.from, exception.to, exception.reason
                );
            }
            ExitCode::SUCCESS
        }
        "boundaries" if args.iter().any(|argument| argument == "--graph") => {
            let crates = boundaries::graph(&root);
            for item in &crates {
                let family = item
                    .family
                    .map(boundaries::Family::label)
                    .unwrap_or("unclassified");
                let dependencies: Vec<&str> =
                    item.dependencies.iter().map(String::as_str).collect();
                println!("{family:<12} {:<32} {}", item.name, dependencies.join(" "));
            }
            println!("{}", boundaries::summary(&crates));
            ExitCode::SUCCESS
        }
        "boundaries" => {
            let crates = boundaries::graph(&root);
            let mut findings: Vec<String> = boundaries::violations_of(&crates)
                .iter()
                .map(|item| item.report())
                .collect();
            findings.extend(
                boundaries::unclassified(&crates)
                    .into_iter()
                    .map(|item| format!("no family: {item}")),
            );
            findings.extend(
                boundaries::depending_on_an_app(&crates)
                    .into_iter()
                    .map(|item| format!("DEPENDS_ON_APP: {item}")),
            );
            findings.extend(boundaries::stale_exceptions(&crates));
            findings.extend(
                boundaries::ledger_authority_outside_the_kernel(&root)
                    .into_iter()
                    .map(|item| format!("LEDGER_AUTHORITY: {item}")),
            );
            findings.sort();
            for finding in &findings {
                println!("{finding}");
            }
            println!("{}", boundaries::summary(&crates));
            if findings.is_empty() {
                println!("boundaries: clean");
                ExitCode::SUCCESS
            } else {
                println!("boundaries: {} outstanding", findings.len());
                ExitCode::FAILURE
            }
        }
        other => {
            eprintln!("xtask: no command called {other}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
