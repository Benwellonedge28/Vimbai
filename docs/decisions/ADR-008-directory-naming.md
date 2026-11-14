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

## Review at close-out (P11.4)

Asked again now that the tree is finished: is a rename cheap enough to be worth it?

It is cheaper than it was - the crates no longer have to move beside a Python service directory of the same name, and
`cargo` does not care about either spelling - but it is still 16 crate paths, 16 package names, every `use` path that
names one, the catalogue, the gate's crate graph and the domain pages. The benefit has not changed: consistency with a
document nobody reads at build time.

Decision: unchanged. The underscores stay. The refusal is now recorded twice, so nobody has to work it out again.
