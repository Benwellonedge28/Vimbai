# ADR-015: A ledger is created when a posting needs it, and control accounts are never invented

**Status:** accepted, 2026-11-25
**Depends on:** ADR-002 (ledger-core is the posting authority), ADR-014 (nothing business-specific is code)
**Amends:** nothing yet - this records the agreed design; the engine still refuses where it used to invent

## The problem

Two shapes have both been tried and both are wrong.

Refusing a posting because its account does not exist yet means a person is asked to choose an account number before
they can record a fact, which is the thing ADR-013 says they should never do. Inventing an account silently is worse:
the chart fills with near-duplicates, and nobody can say which of three "fuel" accounts is real.

## The decision

1. **The type follows from the posting, not from a person.** The side the entry must take decides what kind of account
   is needed - a debit for something owned, a credit for something owed - and the account is created with that type.

2. **Numbering is banded, not guessed.** A new account takes the next free number inside its band: 1xxx assets,
   2xxx liabilities, 3xxx equity, 4xxx revenue, 5xxx to 6xxx expenses. Bands are the chart's own convention and are
   data, not code, so a Book that numbers differently still gets a coherent chart.

3. **Creating and posting is one act, and the account is marked provisional.** An account that exists only because
   something was posted to it says so, so a person can rename it, merge it, or move it.

4. **Reclassification is a transfer entry, never a rewrite.** Moving a balance from a provisional account to a named
   one is a posted entry, so the history of why it moved survives.

5. **Control accounts are never auto-created.** Receivables, payables, tax, suspense, inventory and fixed assets are
   control accounts: other ledgers reconcile against them, so an invented one breaks a reconciliation that cannot then
   be seen to be broken. Where one is missing, the posting is refused and the setting is named - which is the ADR-013
   rule for a missing policy.

6. **One function does the finding.** Every posting path calls the same find-or-create-or-refuse function rather than
   each carrying its own search, so the behaviour cannot drift between journals.

## Why this is not yet in the engine

The engine today refuses a missing account and names it, in every posting path, through its own search. That is the
safe half of this decision and it is what the tests prove. The other half - creating a provisional account and
posting to it in one act - is a change to every posting path and needs the measure work beside it so a
provisional account is visible where a person will look for it. `docs/ROADMAP.md` carries it as open work.

## What this forbids

- A person being asked for an account number in order to record a fact.
- An account created by a posting that is not marked provisional.
- An invented control account.
- Two posting paths with their own account-finding rules.
