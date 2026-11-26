# Record referral awards and partner commissions

Baseline `728f9a22a5371f40fce7278c31566f8a074f2884`. Depends on the verified purchase and refund lifecycle in 07.

## Ownership and events

Use the proposed commercial application module from 07 with the existing platform record boundary `PlatformRecords` in `rust/crates/adapters/database/src/lib.rs`. The old subscription model in `rust/crates/domains/invoicing/src/billing.rs` remains Book-scoped. A referral belongs to the Vimbai subscription business. A partner commission is an obligation Vimbai records for its own administration. Customer ledgers and customer Book exports never carry these records. Payment execution is outside this work package.

Represent ReferralCode, Attribution, AwardPolicyVersion, Award and Commission as explicit records. A public code resolves to an eligible referrer and a policy version with expiry and eligibility. Bind attribution once to the new Vimbai billing subject after confirmation. The policy defines the attribution window, qualifying plan and settlement hold. Use the first eligible, verified paid event from 07 as evidence. Reserve a unique key over the referral policy, referred subject and qualification event. Never grant from a client-side click or signup alone.

Track pending, approved, applied and reversed awards. Add plan credit by sending an idempotent event to the entitlement model in 07. A qualifying refund appends a linked reversal with the original award ID. Partner reporting shows commissions accrued, reversed and marked settled through an authorized administrative record with an external reference. Keep recipient-facing reports limited to totals and attribution references required for support.

## Callers, errors and migration

The public website captures the referral code and forwards a signed or server-held attribution reference into signup. The verified-payment consumer applies the eligibility rules. Customer account views show their own links and awards. The administration view requires a designated commercial role and records policy overrides and reversals. Invalid or expired codes allow signup and show that the link did not qualify. Reject self-referrals and duplicate claims with a clear reason. Existing users start without attribution. Import any previous promises through an audited one-time batch with recorded evidence and an owner approval.

## Acceptance tests

- `a_share_link_grants_only_after_an_eligible_verified_payment` covers signup and a failed transaction.
- `concurrent_payment_events_grant_one_award` runs with repeated provider deliveries.
- `refund_creates_a_linked_reversal_and_preserves_history` checks plan credit and partner totals.
- `self_referral_and_cross_account_award_reads_are_refused` verifies account boundaries.
- Done when the totals reconcile to qualifying payments, effective policy versions are retained, and supported award and reversal events are absent from customer Book exports.
