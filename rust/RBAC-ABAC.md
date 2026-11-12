# vimbai_rbac and vimbai_abac

Two independent crates, both safe Rust (`#![forbid(unsafe_code)]`), both
caller-owned with no globals and no clock: timestamps arrive as microseconds.
They replace `org-authorization-engine` and `policy-engine-service`.

They are deliberately separate. Role grants answer "who is this person here",
attribute rules answer "is this particular thing allowed". A Book needs the first
to render a UI and the second to hold a control, and neither should have to know
about the other.

## vimbai_rbac: roles

A role holds patterns (`invoices:read`, `invoices:*`, `*:read`, `*:*`) and may
inherit other roles. Inheritance is refused when it would close a cycle, and a
role's name is unique inside its organisation.

An assignment ties a person to a role in one organisation, and may carry an
expiry, so a month-end elevation ends by itself instead of waiting for someone to
remember it.

Resolution answers three questions:

* `check(user, org, "invoices:write")` returns the decision shape the service
  returned: `allowed`, the role that granted it, and the reason. The parity
  reasons are preserved, including `No role assignments found` and
  `Permission not granted by any assigned role`.
* `effective_permissions` lists what the person holds, with the roles that
  produced it, so a screen can grey out the right buttons.
* A deny (`role.denying(["invoices:delete"])`) removes a grant even when a broader
  role supplies it, which is how a committee chair withholds one action without a
  parallel role tree.

Deleting a role that is still held is refused with 409 rather than cascading
silently.

## vimbai_abac: attribute rules

A rule is one condition, one effect, one resource type. Effects are allow, deny,
warn and require approval, and the combining algorithm is deny-overrides.

What changed for the better, and why:

* *Operators are validated when the rule is written.* The service accepted any
  string and compared with `if/elif`, so `=>` produced a rule that never fired and
  looked healthy. It is now a 400 with `Unsupported condition operator: =>`.
* *Type mismatches are reported, not swallowed.* A rule that could not compare
  says `type_mismatch` instead of quietly passing. Numeric strings still order,
  because forms and CSV imports send `"15000"`.
* *Every rule carries a reason.* `matched`, `no_match`, `missing_attribute`,
  `type_mismatch`, `action_scope`, `disabled`, `unknown_operator`.
* *Obligations travel with the decision.* A warn or approval requirement does not
  block, but it comes back as `obligations`, so the approvals workflow knows to
  ask.
* *The default is deny.* The service allowed anything no deny blocked, including
  when no rule existed at all. `with_default_effect(Effect::Allow)` restores that
  behaviour for a Book that wants it.
* *Action scoping and priority.* A rule can apply only to `approve`, and
  priorities order the explanation (not the outcome, since deny wins anyway).
* *`exists` and `not_exists`* catch an attribute that should have been present, and
  a null reads the same as an absent key.

Presence rules are how a corridor rule is made safe: `route not_in [...]` cannot
fire on a null route, so it belongs beside `route not_exists -> deny`.

## Where the parity is pinned

`library/rbac/src/tests.rs` and `library/abac/src/tests.rs` cover the original
contracts (decision shape, reasons, deny-overrides, the operator set) and the
additions above. The Python test classes were retired with the services.
