# Platform services

The services that are not about money: who a person is, what the system is doing, and how a Book's data reaches the people and machines that need it.

Crates: `rust/crates/domains/data`, `rust/crates/domains/identity`, `rust/crates/domains/multimodal`, `rust/crates/platform/admin`, `rust/crates/platform/cache`, `rust/crates/platform/documents`, `rust/crates/platform/events`, `rust/crates/platform/identity`, `rust/crates/platform/integrations`, `rust/crates/platform/notifications`, `rust/crates/platform/offline-sync`, `rust/crates/platform/webhooks`

## What it owns

1. Identity and authentication. An account signs in by username or email; a wrong password and an unknown account answer identically, so the door never reveals which accounts exist; an inactive account is refused separately, because that is a different fact; a person with MFA enabled gets a short-lived challenge instead of a session; existing password hashes still verify, so nobody had to be re-invited. The login token keeps the claims clients already read. Enrolment, verification and the five-minute challenge live here too, and both the old and the new addresses answer from the same handlers.
2. Financial identity: the identity a financial institution knows a person by, kept apart from the login.
3. Notifications: the inbox, the single notice, and the read that marks one seen. Notices raised by a completed or failed automation run land here.
4. Documents: filing, search, statistics, linking, and a refusal of duplicate bytes so the same file is stored once.
5. Offline sync: Books and their memberships on the device, including the membership question the gateway asks before it lets a request reach a Book.
6. Events: the live socket a client holds open, and the message bus behind it. The engine holds real sockets, streams a Book's events to the right ones, relays a client's write to the rest of the channel and announces departures; a broker relay fans one publish out to every replica, and a broker outage costs cross-pod reach rather than a failed write.
7. Data: the warehouse, ETL into it, and encrypted backup and restore, where a restore of a backup that never finished is refused.
8. Integrations and webhooks, including the GraphQL door, which mounts a router rather than declaring routes.
9. Admin: the feature categories a Book's administrator can switch.
10. Multimodal pipeline: a capture sits in the inbox until it is corrected, because a captured document that has not been checked is not a record.
11. Cache, which answers its own statistics on its own prefix because a flat statistics address belongs to exactly one service.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `admin-service` | `/admin` |
| `book-sync-service` | `/book-sync` |
| `cache-service` | `/cache` |
| `data-warehouse-service` | `/data-warehouse` |
| `document-service` | `/document` |
| `encrypted-backup-service` | `/encrypted-backup` |
| `etl-service` | `/etl` |
| `financial-identity-service` | `/financial-identity` |
| `graphql-service` | `/graphql` |
| `message-bus-service` | `/message-bus` |
| `mfa-auth-service` | `/mfa-auth` |
| `multimodal-pipeline-service` | `/multimodal` |
| `notifications-service` | `/notifications` |
| `webhook-service` | `/webhook` |
| `websocket-service` | `/websocket` |

## What it does not own

Any financial arithmetic. This page holds no calculator at all: it moves a message, stores a file, or answers who somebody is. Authorisation decisions about a Book belong to the tenancy boundary, and the gateway runs that rule in-process on the device.

## Tests

Fifteen HTTP tests pin the identity door's shipped behaviour, including the identical answer for a wrong password and an unknown account, an inactive account refused even with a valid token, a refresh token spent on use, and a password change retiring every token that owner held. The socket is proven over a real connection, the documents suite covers filing, search, linking and the duplicate-bytes refusal, and the offline sync suite proves a Book survives a full restart.
