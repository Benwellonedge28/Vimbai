# Referral attribution and rewards
Status: proposed.

## Model
Each eligible account receives an opaque shareable referral code and HTTPS link. Referral record stores referrer, referred account, campaign, attribution time, consent and qualification state. No personal identifier in public link. Landing page preserves attribution through signup; installation alone does not prove a unique eligible user. Use platform-supported deferred linking where available and a manual code fallback.

## Lifecycle
Click → signup attribution → qualification → pending reward → approved grant → expiry or reversal under published terms. First valid attribution is proposed default; define window, eligible events, reward tiers and caps in versioned campaign policy. Reward may unlock features or extend subscription; no guarantee of cash. Never reward self-referrals, duplicate accounts or fraudulent transactions. Use proportionate abuse review and appeals.

## Integration
Monetisation referrals owns attribution and reward ledger; identity emits verified signup event; subscriptions emits qualifying payment event; entitlement evaluator applies grants; Flutter share sheet and website use same link. Worker handles qualification and expiry. Do not require contacts access or unsolicited messaging.

## Acceptance
Cross-device signup, repeated click, duplicate event, refund, competing codes, expired campaign, fraud appeal, privacy deletion and reward reconciliation. Report qualified conversion rather than raw downloads.
