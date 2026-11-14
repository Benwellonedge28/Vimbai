# ADR-008: Crate directory names are not renamed for consistency

*Status:* accepted, 2026-11-14.

## Context

The proposed tree hyphenates calculation crate directories: `cost-accounting`, `capital-valuation`, `ratios-statements`.
The repository uses underscores: `cost_accounting`, `capital_valuation`, `ratios_statements`, and the same pattern in
`equity_payroll`, `group_accounting`, `market_risk`, `performance_measures`, `tax_reporting`, `treasury_fx`.

Renaming them would touch 16 crate paths, their `Cargo.toml` names, every workspace member entry, every `use` path that
names a package, the catalogue and any documentation that references a path.

## Decision

1. Crate directories keep underscores. Underscores are the Rust package convention; the hyphenated form in the proposal is
   a directory-name preference, not a technical requirement.
2. A rename is not performed for cosmetic alignment, at any point in this roadmap.
3. If a crate is renamed, it is because the crate's *responsibility* changed, and the rename is part of that change.

## Consequences

- This repository will differ from the proposed tree in 16 path names, permanently, and that is recorded here rather than
  rediscovered each time somebody diffs the two.
- Rust package names, import paths and the catalogue stay stable, so nothing that consumes them has to move.
