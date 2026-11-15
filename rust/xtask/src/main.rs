//! Repository tooling.
//!
//! What lives here is the work the Python scripts did that must outlive them: the generators that keep a derived file
//! honest, and the gates that fail when the estate and its description disagree. It is a binary rather than a crate of
//! helpers because each of those jobs is something a person runs and reads the output of, and because a test can call
//! the same functions and assert the answer.
#![forbid(unsafe_code)]

use std::process::ExitCode;

use xtask::{
    boundaries, catalog, documentation, domains, flutter, gateway, makefile, manifests, openapi,
    owners, published, reachability, readme,
};

const USAGE: &str = "\
xtask - repository tooling

USAGE:
    cargo run -p xtask -- <command>

COMMANDS:
    catalog            rewrite catalog/ from the gateway's routes and catalog/ownership.json
    catalog --check    report whether the committed catalogue is current, and exit 1 if it is not
    gateway            report the addresses the gateway publishes that the engine does not answer
    gateway --addresses  print every address the engine answers, one per line
    gateway --misrouted  print every routed prefix the engine answers while its entry forwards elsewhere
    gateway --ports  print every entry whose stated port is not the port its address names
    openapi            rewrite contracts/openapi/v1/domains/ from the engine's routes
    openapi --check    report an address with no contract, a contract with no address, or a stale file
    owners             print which crate answers each prefix the catalogue no longer routes
    domains            report a routed service with no domain page, and a page that is stale, and exit 1
    reachability       report the modules no door can reach, one per line
    readme             check the front page's figure and citations against the catalogue and the decisions
    docs               check the index, the counts, the references and the tests a page names
    makefile           check every target is .PHONY, described by `make help`, and runs a command xtask has
    boundaries         report crate dependency rule violations, and exit 1 if there are any
    boundaries --graph print every crate with its family and its internal dependencies
    boundaries --rules print the rule table as the documentation states it
    boundaries --exceptions print the tolerated dependencies and why
    routes             rewrite docs/routes-snapshot.json from the addresses the engine answers
    routes --check     report whether the committed snapshot is current, and exit 1 if it is not
    manifests          report a cluster, compose or image declaration that does not hold, and exit 1 if any
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
        // Which domain answers the addresses the catalogue no longer has a service for.
        "owners" => {
            let attributed = owners::attributed(&root);
            for (prefix, owner) in &attributed {
                println!(
                    "{prefix}\t{}\t{}\t{}",
                    owner.krate, owner.domain, owner.addresses
                );
            }
            println!(
                "owners: {} prefix groups attributed to a crate",
                attributed.len()
            );
            for (prefix, crates) in owners::split(&root) {
                println!(
                    "split: {prefix} is answered by {}",
                    crates.into_iter().collect::<Vec<_>>().join(", ")
                );
            }
            ExitCode::SUCCESS
        }
        "openapi" if args.iter().any(|argument| argument == "--check") => {
            let found = openapi::problems(&root);
            for finding in &found {
                println!("{finding}");
            }
            if found.is_empty() {
                println!("openapi: every published address has a contract, and every contract is answered");
                ExitCode::SUCCESS
            } else {
                println!("openapi: {} outstanding", found.len());
                ExitCode::FAILURE
            }
        }
        "openapi" => match openapi::write(&root) {
            Ok(written) => {
                println!("openapi: {written} domain specifications written");
                ExitCode::SUCCESS
            }
            Err(problem) => {
                eprintln!("openapi: {problem}");
                ExitCode::FAILURE
            }
        },
        "domains" => {
            let found = domains::audit(&root);
            for finding in &found {
                println!("{finding}");
            }
            if found.is_empty() {
                println!("domains: every routed service has one page, and every page is true");
                ExitCode::SUCCESS
            } else {
                println!("domains: {} outstanding", found.len());
                ExitCode::FAILURE
            }
        }
        "flutter" => {
            for feature in flutter::features(&root) {
                println!(
                    "feature {}: {}",
                    feature.name,
                    if feature.parts.is_empty() {
                        "no parts yet".to_string()
                    } else {
                        feature.parts.join(", ")
                    }
                );
            }
            let left = flutter::leftovers(&root);
            let found = flutter::findings(&root);
            for finding in &found {
                println!("{}: {}", finding.file, finding.detail);
            }
            println!(
                "flutter: {} dart files, {} still in a layer folder, {} outstanding",
                flutter::dart_files(&root).len(),
                left.len(),
                found.len()
            );
            if found.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        // P10.5: the estate, as declared. The cluster is base plus overlays, compose names only what exists, and every
        // image is built by something.
        "manifests" => {
            let found = manifests::manifest_problems(&root);
            for finding in &found {
                println!("{finding}");
            }
            if found.is_empty() {
                println!("manifests: the cluster, the compose estate and the images agree with the repository");
                ExitCode::SUCCESS
            } else {
                println!("manifests: {} outstanding", found.len());
                ExitCode::FAILURE
            }
        }
        "readme" => {
            let problems = readme::readme_claims(&root);
            for problem in &problems {
                println!("{problem}");
            }
            if problems.is_empty() {
                println!("readme: the front page agrees with the catalogue and cites decisions that exist");
                return ExitCode::SUCCESS;
            }
            ExitCode::FAILURE
        }
        "makefile" => {
            let problems = makefile::makefile_claims(&root);
            for problem in &problems {
                println!("{problem}");
            }
            if problems.is_empty() {
                println!("makefile: every target is .PHONY, described by `make help`, and runs a real command");
                return ExitCode::SUCCESS;
            }
            ExitCode::FAILURE
        }
        "docs" => {
            let problems = documentation::documentation_index(&root);
            for problem in &problems {
                println!("{problem}");
            }
            if problems.is_empty() {
                println!("docs: the index names every document, every path exists, and every count is the count");
                return ExitCode::SUCCESS;
            }
            ExitCode::FAILURE
        }
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
        "gateway" if args.iter().any(|argument| argument == "--misrouted") => {
            let findings = gateway::misrouted(&root);
            for finding in &findings {
                println!("{finding}");
            }
            if findings.is_empty() {
                println!("every prefix the engine answers forwards to the engine");
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        "gateway" if args.iter().any(|argument| argument == "--ports") => {
            let findings = gateway::port_drift(&root);
            for finding in &findings {
                println!("{finding}");
            }
            if findings.is_empty() {
                println!("every entry's stated port is the port it forwards to");
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
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
        "routes" => {
            let check = args.iter().any(|argument| argument == "--check");
            match published::write(&root, check) {
                Ok(()) => {
                    if check {
                        println!("routes: current");
                    } else {
                        let found = published::addresses(&root).len();
                        println!("wrote docs/routes-snapshot.json: {found} addresses");
                    }
                    ExitCode::SUCCESS
                }
                Err(problem) => {
                    eprintln!("{problem}");
                    ExitCode::FAILURE
                }
            }
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
