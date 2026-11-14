# Deprecation policy

How an address stops being answered without a client discovering it by failing.

## The four steps, in order

1. **Announce.** The contract marks the address `deprecated: true` with a `description` naming its replacement and the
   date it stops answering. A deprecation nobody can read in the contract is not an announcement.
2. **Answer both.** The old address and the new one answer the *same* implementation, not two copies. This is the step
   that gets skipped and the reason clients break: a second implementation under the new name is a second answer waiting
   to disagree, and the deprecation window is when nobody is looking.
3. **Wait.** A full release cycle, at least one published version, and no shorter than the date in the contract. During
   the window the old address is a promise and the HTTP tests walk it.
4. **Remove, in a version.** Deleting an address is a breaking change, so it lands in a new API version, and the deletion
   removes the contract entry, the route and the test together. A route removed while its contract remains fails
   `openapi --check`; a contract removed while the route remains fails the same gate from the other side.

## What this repository has already done, and what it means

The retirement of the Python services was one long deprecation handled without a version bump, because no client URL
moved: a service's published prefix was repointed at the Rust engine, and the engine answered every address the service
had published. Where an address genuinely disappeared, it was because nothing had ever answered it - a published path
with no handler is not a deprecation, it is a defect that was fixed.

The 180 prefix groups in `contracts/openapi/v1/domains/unattributed.yaml` are the visible debt from that work: addresses
that answer but that no catalogue service claims. They are documented rather than deprecated, because they are still
promised. Attaching each to a catalogue owner is the work that lets a later deprecation be a decision instead of an
archaeology exercise.

## Internal contracts

A module, a crate or a bus topic inside the repository follows the same four steps with a shorter wait, because the
consumer is in the same workspace and the compiler will say so. The rule that survives the shorter wait is step two: the
old name and the new name call one implementation until the old one is gone.
