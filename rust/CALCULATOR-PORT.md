# Calculator port tracker

Generated from the tree.

Rules for every port in this file: safe Rust only, money carried as `vimbai_common::Money`
(decimal, exact at any magnitude), amounts published as decimal strings from the library.
`library/HTTP.md` describes the surface that makes deletion possible; `scripts/retire_gate.py`
says which services have passed every check. 169 stateless calculators; 45 answer through the Rust library today, 17 of them served at /calc/.

Services that own records are ported into the Rust store and served from the API's record
routes; they are listed here too, because the Python service is no longer the engine behind them.

## Wired to the Rust library

- accounting-service -> accounting crate, whole. Every route the service publishes is answered by the
  Rust engine, twenty of them as the core book: the chart of accounts (create, read by number, list,
  update, delete, period activity) and the journal (create with double-entry validation and a hash-chain
  stamp, read, list, delete, integrity), plus the ledger, trial balance, income statement and balance
  sheet computed from those entries. Amounts exact end to end, so a trial balance over
  123456789012345678901.55 keeps every digit.

  The rest are the books of original entry and the working papers beside the ledger. `subsidiary_journals`
  holds the six journals and the vendor bill, each recording its document and posting the double entry it
  implies in one act, so the record carries the id of the entry it generated; `bookkeeping` holds petty
  cash, bank reconciliation, statements of affairs, capital calculations, control accounts, receipts and
  payments, and the single-entry conversions.

  Shapes pinned rather than repaired: only `posted` entries move anything; an account is debit-normal only
  when its normal balance says `debit`; a trial balance moves a negative balance to the other column and
  calls itself balanced only on exact equality; the income statement reads period activity and drops an
  account whose movement runs against its natural side; the balance sheet lists every account of its three
  types and carries no profit for the period, so it can fail to balance; deleting an account is refused
  while a line touches it; a reference number is unique per source module only for the six modules the
  service checks. The journals post to the service's own fixed chart (`1100`/`4000` for a sale,
  `1200`/`2000` for a purchase, and so on), so a book whose chart lacks those numbers refuses the document
  instead of guessing where it belongs; tax is recorded on a document but never posted; a purchase return
  debits its expense account, raising it; a cash receipt books every source that is not a cash sale to
  `4100`; and both ageing ledgers owe the whole document with nothing paid against it, calling it overdue
  past thirty days whatever it was recorded as. The fixed-assets and inventory subsidiary ledgers answer
  with nothing at all, because they were placeholders and no figures are invented for them. A capital
  calculation given neither a profit nor a loss reports its opening capital as the closing one; a control
  account totals only the entry types it knows and reports each entry's balance one step behind itself; a
  receipts and payments account moves the bank balance only, leaving cash where it started; and petty cash
  vouchers never move the fund's imprest. A single-entry conversion and a profit estimation ask the same
  question and now call one calculation instead of two copies of it.

  Two deliberate changes: a posted entry cannot be edited or deleted, because the old update path silently
  broke its own stamp and every stamp after it, and corrections belong in reversing entries; and a
  reconciliation's per-line running balance is the running book balance, where the service reported the
  account's debit total less its credit total before each line, which is not a running balance at all. The
  shipped receivable and payable ledgers filtered on a date belonging to a node their query never matched,
  so the filter did nothing; the port filters on the document's own date.
- accounting-standards-service -> accounting crate. The catalogue is data, not records: fourteen
  standards and their requirements lifted from the service's own definitions into
  `crates/domains/accounting/assets/accounting-standards.json`, so nothing in it was retyped by hand.
  What a caller owns lives on the store and is Book-scoped: an organization's configuration, its account
  mappings, its accounting policies and its compliance history. Shapes pinned: configuring an
  organization replaces its configuration, and a policy is replaced per organization, standard and area;
  activating a standard already active (or deactivating one absent) writes nothing and answers as if it
  had; an unknown standard is refused but its requirements answer as an empty list; and a compliance
  check writes one check per requirement, passing unless the caller's validation results say otherwise,
  replacing the earlier history rather than appending to it.
- departmental-accounting-service -> accounting crate, records on the store: departments and their
  cost centres, allocation rules, cost pools, the allocation run, inter-department bills and their
  approval, the department financial summary and performance report, and the comparison and
  cost-distribution reports. Shapes kept as shipped and pinned: a pool's `included_departments` is never
  consulted (every active department it does not exclude is a target); the headcount basis looks at no
  figure at all and the expenses basis measures one department against a total made of that same
  department, so both land on an equal share; the revenue basis takes its figures from what the caller
  supplies; a second run of a pool replaces the first and closes it; the budget variance is budget less
  revenue, not less spending; and approving a bill returns its journal entry rather than posting it.
  Revenue, direct expenses and budget totals used to come from other services over HTTP and are now
  supplied by the caller, absent meaning zero.
- company-accounting-service -> accounting crate, records on the store: companies and the four equity
  accounts each one needs, the shareholder register, share capital, capital transactions, dividends and
  their per-shareholder payments, retained earnings, reserves, and the three reports (equity statement,
  shareholder register, dividend history). Shapes kept as shipped and pinned: registering a holder
  recalculates every holding in the company and leaves them alone when the total is zero; paid-up capital
  is issued shares times paid-up value, with premium carried separately; a dividend pays only holders of
  its own class at a flat ten percent withholding whatever the dividend itself declared; the equity
  statement takes the newest share capital and retained-earnings records. Journals are returned rather
  than posted, because posting belongs to whoever owns the ledger.
- the shareholder register has one writer: this port and issued-share-capital-service write the same
  `:ShareholderNode` rows through one shared shape in the store crate (accepting `name` or
  `shareholder_name`), and the equity route now registers through this port so a holding always has its
  percentage recalculated.
- cashbook-service -> accounting crate, records on the store: cash accounts (a code used once per
  Book), entries that move the account balance as they are written, posting and voiding, the account
  summary, bank reconciliations including the outstanding items, cash-flow entries with their summary,
  and the cash position with its history. Shapes kept as shipped and pinned: a new account opens at its
  opening balance and ignores any balance sent with it; an entry is always stored unreconciled; the
  summary's totals and closing balance count posted entries only while its transaction count covers
  every entry; posting and voiding do not check the entry's current state, so a voided entry can be
  posted and a second void reverses the balance again; the cash-flow summary's variance is expected
  minus actual while its net change is actual minus expected; and the position history runs from zero
  rather than from any opening balance.
- ifrs-reporting-service -> accounting crate, records on the store: reports limited to the five kinds,
  the filtered listing, the payload kept as given, disclosure notes, and approval. Adding a note also
  appends its number and title to the report's own disclosure index, so the report carries its notes;
  approval is from draft or reviewed only; and notes for a report that does not exist list empty while
  reading that report refuses. The IFRS calculators (revenue, share-based payment, instruments) stay in
  their own crates: this is the paperwork, not the arithmetic.
- intercompany-service -> accounting crate, records on the store: group entities, the transactions between
  them limited to the five known types, the filtered listing, matching a pair, and the eliminations.
  Four shapes kept as shipped and pinned: a match is exact equality with no tolerance and no currency
  check, so 100 USD matches 100 EUR; matching never checks the legs face each other, so two
  same-direction transactions match and eliminate with both sides reading from the same entity; and a
  refused transaction type or mismatched amount answers 422, the record routes' own status for refused
  input, rather than the original 400.
- cash-flow-service -> accounting crate, records on the store: a project's yearly inflows and outflows as
  entries, the totals, the kept schedules, and the three standalone flow calculators (totals, net flows
  with the shorter side padded, cumulative running total). Two shapes kept as shipped: the initial
  investment is recorded and never subtracted, so a project's net flow ignores what it cost to start;
  and reading a schedule that does not exist answers 200 with an error body rather than 404.
- balance-sheet-service -> accounting crate, records on the store: the sheet with its three totals computed
  server side, the history, the latest, and the ratios read off it. Balances to within a cent; every ratio
  denominator floored at 1, and no sheet at all is a 404 rather than an empty sheet. The float copy in
  the ratios crate was deleted with this: nothing reached it from any door, so it was a second set of
  balance-sheet arithmetic drifting from these figures.
- fund-accounting-service -> accounting crate, records on the store: funds with their derived net assets,
  the transactions recorded against them, and the company total. A transaction recomputes income and
  expenses from every transaction the fund has rather than adding itself on, and never moves the
  fund's balance - so a fund that receives a thousand shows more net assets and the same balance.
  Pinned as shipped rather than repaired, because a fund already on record would otherwise change.
- revenue-recognition-service -> accounting crate, records on the store: contracts with their obligations,
  the allocation of the transaction price by relative standalone selling price, recognition against one
  obligation at a time capped at what it is worth, and the company summary. Two shapes kept as shipped:
  an obligation with no standalone selling price keeps the price it was given, so the allocated parts
  need not add back to the total; and a recognition with no amount recognises the whole remaining
  obligation rather than nothing.
- capital-reconstruction-service -> equity crate, records on the store: the scheme with its reduction,
  its adjustments, its reserve conversions, and the approval entry that writes losses off against the
  reduction and balances to retained earnings. Only write-offs are posted; a transfer's new balance is
  stored but does not enter the entry, and approval is not guarded against running twice - both pinned
  rather than repaired.
- debentures-service -> equity crate, records on the store: classes (nominal, issue price, coupon,
  redemption price), issues booking the discount or premium, per-period interest with a flat twenty
  percent withheld, payment of it, and redemptions booking the premium. Accrual takes the outstanding
  count from the caller rather than the class, as the service did; an unrecognised payment frequency is
  treated as monthly.
- issued-share-capital-service -> equity crate, records on the store: issues, the shareholder register,
  and the capital summary. The issue entry credits share capital at one unit a share because the service
  hard-coded that nominal regardless of class, so preference and ordinary issues post the same capital
  and an issue below one unit posts negative premium; pinned by a test rather than repaired.
- initial-investment-service -> equity crate, records on the store for `/create` (the breakdown, total and
  net of scrap), with `/simple` and `/with-capital-allowance` as pure arithmetic that stores nothing.
  Zero lines are left out of the breakdown but still counted; scrap value never enters the simple outlay.
- share-redemption-service -> equity crate, records on the store: a redemption initiated, funded by a
  fresh issue, then completed, with the reserve requirement in between. Redeeming out of proceeds sets
  up a requirement for the nominal value and starts `awaiting_crr`; a fresh issue funds it and creates
  no reserve; every other method creates the nominal value. Exact throughout.
- share-premium-service -> equity crate, records on the store: premium entries computing (price less
  nominal) times shares, utilizations on four kinds, signed adjustments, and a balance of received less
  used plus adjusted. An issue below nominal records a negative premium rather than refusing.
- capital-redemption-reserve-service -> equity crate, records on the store: redemptions creating the
  reserve as proceeds less nominal (preference debits 3205, everything else 3200), manually created
  reserves from a capital reduction, utilizations, and a summary counting creations and redemptions as
  one figure.
- general-reserve-service -> equity crate, records on the store: reserves with their allocations and
  utilizations, the balance the single figure they share. A utilization past the balance is refused;
  a minimum or target balance is recorded and never enforced, as the service did.
- retained-profits-service -> equity crate, records on the store: appropriations with their
  carry-forward chain (total appropriated, carried forward, closing), interim dividends and prior-year
  adjustments; the summary adds carried forward to the adjustments for one company only.
- bonus-shares-service -> equity crate, records on the store: bonus issues and the reserve each
  capitalises, with the released amount kept as the total capitalised.
- right-issues-service -> equity crate, records on the store: rights offers with their acceptances and
  renunciations. An issue completes when every entitlement is covered, counting one unit per
  renunciation as the service did.
- ordinary-shares-service -> equity crate, records on the store: dividends declared and paid against
  the shares in issue, as one implementation shared with the preference path.
- preference-shares-service -> equity crate, records on the store: preference classes, their issuances
  and redemptions, dividends with arrears accumulated on the declared total. Journal entries are built
  and returned, not posted.
- authorized-share-capital-service -> equity crate, records on the store: share classes, their
  issuances and buybacks, and the used-capacity summary. The ceiling is enforced on issue and the
  issued count on buyback.
- disposal-group-service -> assets crate, records on the store: groups, their assets, impairment
  tests and status changes. Adding an asset twice updates it instead of counting it twice.
- lease-termination-service -> assets crate, records on the store: terminations, settlements,
  approve and complete. The settlement is exact and the approval order is kept.
- fixed-assets-register-service -> assets crate, records on the store: register, list, get,
  depreciate, entries, dispose, summary.
- intangible-assets-service -> assets crate, records on the store: register, list, get, amortize,
  amortisation entries.
- revaluation-reserve-service -> assets crate, records on the store: revaluations, utilizations,
  the cumulative reserve. Journal entries are built and returned, not posted.
- fixed-assets-schedule-service -> assets crate, the same register plus its schedule doors:
  partial update, schedule generate and latest, the scheduled summary.
- biological-assets-service -> assets crate, six routes from `biological-assets-measure` to
  `biological-assets-disclosure-template`.
- disposal-account-service -> assets crate, `disposal-account-calculate` and `disposal-account-post`.
- investment-property-service -> assets crate, five routes from `investment-property-fair-value-model`
  to `investment-property-yields`.
- absorption-costing-service
- job-costing-service
- activity-based-costing-service
- actual-cost-service
- amortization-service
- appropriation-control-service
- asset-turnover-service
- balanced-scorecard-service
- budgeted-cost-service
- business-valuation-service
- capital-expenditure-budget-service
- cash-flow-statement-service
- cash-optimization-service
- consolidation-service
- corporate-tax-service
- croic-service
- depreciation-service
- divestiture-service
- du-pont-service
- eps-service
- equity-changes-service
- eva-service
- financial-distress-prediction-service
- fixed-assets-service
- fraud-risk-assessment-service
- goodwill-service
- government-grants-service
- group-tax-service
- internal-control-evaluation-service
- inventory-valuation-service
- net-realizable-value-service
- options-pricing-service
- partnership-accounting-service
- partnership-sale-service
- payment-optimization-service
- pension-accounting-service
- process-costing-service
- product-costing-service
- regulatory-ratio-service
- basis-apportionment-service
- cvp-analysis-service
- overhead-absorption-rate-service
- selling-price-reduction-service
- standard-cost-service
- substantive-testing-service
- target-costing-service
- tax-provision-service
- tax-risk-service
- throughput-accounting-service
- trading-account-service
- treasury-analytics-service
- vat-reporting-service
- insurance-claims-service
acquisition-financing-service
asset-allocation-service
audit-evidence-service
business-combination-service
business-documents-service
capital-allocation-service
commodity-hedging-service
corporate-governance-service
cost-accounting-service
currency-hedging-service
due-diligence-service
esg-reporting-service
financial-planning-service
foreign-currency-translation-service
futures-hedging-service
going-concern-service
investment-appraisal-service
lease-management-service
management-dashboard-service
mva-service
operational-risk-service
portfolio-optimization-service
project-accounting-service
provisions-contingencies-service
ratio-analysis-service
segment-reporting-service
statutory-filing-service
trial-balance-service
xbrl-reporting-service

- workflow-service -> workflow domain crate (rust/crates/domains/workflow, which is the doc's domains/workflow),
  records on the store: definitions and the instances that walk them. A definition holds its steps IN ORDER, each step
  naming the steps that follow it and, optionally, the ones a rejection goes to instead, and it is active unless it is
  not. An instance starts at the FIRST step in the definition's list - the shipped engine took the first step rather
  than a step named as the start - and a definition with no steps at all, or one that has been switched off, has no way
  in. The first step's role, person and configuration become the first task, which arrives pending, and the instance is
  running. Completing a task records the action it was given (approve, reject or complete), who completed it, when and
  any comment; the task leaves the current steps and the steps that follow arrive as new pending tasks. SHIPPED QUIRK
  PINNED: a rejection follows the rejection route only when the step named one - an empty rejection route is no route,
  so a rejection falls through to the ordinary next steps, which is what the shipped truthiness test did. A task that is
  not waiting cannot be completed twice, and the refusal names the state it is already in. An instance is complete once
  nothing is current, with its end stamped, and one that was failed or cancelled stays that way even when further tasks
  appear. The clock is taken from the request, so a walk can be reasoned about and tested exactly. Tasks are held as a
  property list on the instance, the same shape journal lines and report rows already use. Routes:
  /workflow-definitions (+ /{id}, /{id}/start) and /workflow-instances (+ /{id}, /{id}/complete), all new.

- cache-service -> cache crate (platform shelf), records on the store: keyed entries with a life, a size limit, and
  the counts a cache keeps of itself. An entry is read before its moment and answers its value with how long is left;
  read after it, it is DROPPED and the read answers as a miss, so a stale value is never handed back. A hit marks the
  entry as the most recently used, and when the cache is full it is the least recently used entry that goes, not the
  oldest written (pinned by a test that reads the older entry and watches the newer one evict). Writing the same key
  again replaces rather than holds two, and answers the value with the life it was given. Removing says whether there
  was anything to remove; clearing says how many went. The counts are the cache's own - hits, misses, evictions, writes,
  and the hit rate as a percentage of reads, with 66.67 pinned for two hits in three reads and 0 rather than a made-up
  figure for a cache nobody used. Modelling points: the clock comes from the request (`now`, in seconds) rather than
  being read here, so a cache can be reasoned about and tested exactly; entries and counts live on the store rather than
  in one process's memory, so every replica sees the same cache, which the shipped in-memory OrderedDict could not do.
  Routes: /cache (write, clear), /cache/{key} (read, remove) and /cache-stats, because /stats is the receivables control
  account.

- webhook-service -> webhooks crate (platform shelf), records on the store: the endpoints a Book subscribes, the
  events dispatched to them, and how each delivery went. An endpoint is registered for a company with a URL, the events
  it wants and a secret; the answer is its id, its URL and its events, never the secret. A dispatch goes only to the
  caller's own endpoints for that company that are switched on AND subscribed to that event, and writes ONE delivery for
  each: under 400 is delivered, 400 or over is failed, and one that could not be reached at all is failed with the
  reason kept rather than a code invented. Each delivery remembers its single attempt, the code it got and when it last
  tried. A signed endpoint gets an HMAC-SHA256 over the body exactly as sent in X-Signature-256; an endpoint with no
  secret is sent unsigned rather than signed over nothing. Deliveries list newest first, fifty by default. Reaching the
  outside is not the crate's business: a Sender carries the request, so the service is checked without a network.
  NAMED LIMITATION: this build has no TLS client, so an https endpoint is reported unreachable and the delivery recorded
  as failed with that reason (the api-server's PlainHttp sender posts plain http directly). Closing it means swapping
  that one type for a TLS client; nothing else in the service changes.

- notifications-service -> notifications crate (platform shelf), records on the store: each person's own inbox,
  their preferences, and the five templates a notice is filled from. A notice is written once PER RECIPIENT and each
  copy belongs to that recipient, so a notice raised by one person lands in another's inbox and is theirs to read or
  delete; every copy arrives already sent, with the moment it went, and remembers the sender it came from. An inbox is
  the caller's own: asking for somebody else's is refused outright (403) rather than answered empty, and marking
  everything read counts only what was still unread. Preferences are personal too, and a person who has never set any
  reads the defaults - no channel choices, email batched every sixty minutes, no quiet hours; saving again replaces
  rather than piles up. Five templates come with the service: a notice built from one has its {{variable}}
  placeholders filled in, and a name the service does not know is missing. A workflow notice is named after its event
  and is filed as a COMPLETED workflow only when it completed - everything else reads as a failure, which is what the
  shipped two-way mapping did - while an approval notice is loud only when it is the request itself and goes to the
  approvers, with every answer afterwards going to whoever asked. Nobody is shown as listening, because the shipped
  service broadcast each notice over a live socket and none exist here. Modelling point: because copies live in one
  Book beside other people's, each listing and single-record answer filters on the copy's OWNER rather than trusting
  the scope alone. Routes: /notifications, /notifications/batch, /notifications/{id} (read and delete), the two read
  paths, /preferences/{user_id}, /notification-templates and its two, /workflow/{id}/notify, /approval/{id}/notify and
  /notification-stats - /templates belongs to the report service and /stats to the receivables control account. The
  store gained a Forbidden answer (403) for the personal-inbox case.

- alerts-service -> alerts crate (platform shelf), records on the store: the rules a Book watches and the alerts those
  rules raise. A rule's name is three to a hundred letters, its category, severity and action are the ones the service
  names, and its cooldown is a whole number of seconds that may be nothing (anything else is refused, not read as
  nought); a rule arrives enabled with a trigger count of nought, and replacing it keeps the bookkeeping - how often it
  has fired and when it last did - so an edited rule does not look like a rule that never fired. A condition is a
  threshold (field, operator, value, all three required, the operator one of gt/gte/lt/lte/eq/neq compared as exact
  decimals), a pattern (a velocity: more than the threshold inside an hour, and no longer a window) or a deadline
  (within days_before of now, seven when nothing is said, and a deadline already gone is NOT approaching). Anything
  else, an unknown operator, a field the data does not carry, or a value that is not a number is not a match - the
  shipped engine answered false rather than raising. Evaluating watches only ENABLED rules and holds a rule back while
  its cooldown has not run out; a firing rule raises an alert titled after the rule, counts itself fired and remembers
  when. Triggering a rule by name fires whatever its cooldown and whether or not it is switched on, because the shipped
  endpoint was for other services to shout through. An alert is raised active and is acknowledged, resolved or
  dismissed whatever status it held, as the shipped endpoints never looked. Two limits of where this runs: the shipped
  service broadcast each alert over a live socket and there are no live sockets here, so a statistics answer reports no
  listeners; and "now" is taken from the request rather than a clock so a cooldown or a deadline can be reckoned over.
  Routes: /rules, /rules/{id}, /alerts, /alerts/{id}, the three settle paths, /alerts/evaluate (since /evaluate is an
  access decision) and /alerts/stats (since /stats is the receivables control account).

- document-service -> documents crate (platform shelf), records on the store: the files a Book holds, their checksums,
  the text taken off them, and what they are linked to. A file's KIND is judged by its extension alone against the ten
  allowed, and anything else is refused before anything is stored; the content type follows the extension, an unlisted
  one reading as a plain byte stream. The same CONTENT cannot be filed twice - the checksum is taken over the bytes and
  a document already held under it refuses the second copy as a duplicate - while a document that has since been
  deleted no longer blocks a fresh copy. Deleting is SOFT (the record keeps its place, marked deleted, with the moment
  it went) unless the caller asks for it permanently, which also drops the stored content; downloading a deleted
  document is refused, and a preview exists only for pictures. A listing leaves deleted documents out, newest first; a
  search adds a type, ANY of the tags named, a date range on when it was filed, what it is linked to, and a phrase
  matched against the title, the description AND the text taken off the document, so a document found by its own OCR
  text answers that search. The text itself is a PLACEHOLDER as the shipped service's was: a picture yields text and
  three entities, a PDF yields text, anything else yields nothing with a confidence of zero. Deviation: the shipped
  service kept bytes on disk under a generated name; a service crate has no disk, so the bytes travel as base64 in the
  record (the standing rule for binary content in JSON transport) and the download hands them back the same way, with
  the checksum still taken over the bytes so the same content refuses a second copy either way. Routes answer under
  /files and /document-statistics: /documents is the financial document state machine's, whose records are a different
  thing, and /statistics already answers the receivables control account.

- zero-trust-data-service -> security crate, records on the store: the access policies a Book sets and every decision
  taken against them, kept as a record so an access question can be answered after the fact. A policy is the caller's
  own, replaced WHOLE when updated (the id asked for wins over any id sent with it) and defaulting to the lowest
  clearance gate and to requiring MFA; deleting one that is not the caller's answers as missing rather than forbidden.
  An evaluation is decided against the OLDEST policy naming the resource, so a second policy for the same resource is
  unreachable until the first is deleted - the shipped service took the first of a list it scanned in order, and a
  stable rule is better than whatever order the graph hands back. A policy that requires no roles lets anyone through
  that gate; a required role is satisfied by holding ANY of them; a required clearance is a FLOOR rather than a match,
  and a name the scale does not know counts as the lowest level, which is why a policy's default "standard" is the same
  gate as "public". MFA and the address list bite only when the policy asks. Every decision is recorded with its
  reasons joined ("MFA required but not verified; IP not in whitelist"), or "Access granted". The SUBJECT of a decision
  is not the caller, so it is stored under its own name and presented back as user_id. Decisions are listed oldest
  first, the last `limit` of them, fifty when nothing was said. Routes: /access-policies (both methods, since /policies
  already answers the policy register), /access-policies/{id}, /evaluate, /attempts and /attempts/user/{id}.

- benefits-admin-service -> payroll crate, records on the store: the benefit plans a Book offers, who is enrolled in
  them, and the leave each person has accrued. A plan's kind is one of five (pension, medical, dental, life insurance,
  leave) and anything else is refused by name, as a validation refusal (422); a plan arrives active. An enrollment
  needs a plan the CALLER holds - another Book's plan answers as missing - and the same person cannot be enrolled in
  the same plan twice, which is a 409 rather than a second record. Leave is a RUNNING BALANCE rather than an entry:
  each accrual carries forward the balance of the last accrual for that person and that kind, adds the days accrued
  and takes off the days taken, so an interest-only month never pushes the balance anywhere odd. Days are reckoned
  exactly rather than in floats, a movement of no days is refused (the shipped endpoint's gt=0 constraint), and a
  person's leave is listed newest first, by kind when one is asked for. Routes: /benefit-plans (both methods),
  /enroll, /employee/{id}/enrollments, /leave/accrue and /employee/{id}/leave. Note: the shipped collection path was
  /plans, which the subscription plan catalogue already answers, so the benefit plans answer at /benefit-plans.

- subscription-plans-service -> invoicing crate, records on the store: the plans a Book sells on and the
  subscriptions taken against them. A plan keeps the tier, name and monthly price it is given and defaults to 5 users,
  1 company and 1,000 calls a month when nothing else is said; its price is read as an exact decimal, so a plan priced
  199 is 199 rather than a float that has drifted. A subscription needs a plan the CALLER holds - another Book's plan
  is simply not found - and takes the plan's TIER rather than a tier of its own, with its period running 30, 90 or 365
  days from the day it starts. The upgrade preview is PURE and reads no stored plan: it prices the four tiers from a
  fixed table (0, 49, 199, 999 a month), which is what the shipped endpoint did, so a plan a Book priced itself does
  not change what an upgrade is quoted at. Proration is the daily difference over the days left in the period,
  reckoned in exact money and rounded to cents, and a period end that cannot be read prorates nothing. Routes:
  /plans (both methods), /subscribe, /subscriptions/{company} and /upgrade. "Today" comes from the caller rather than
  the clock, so a quote can be repeated.

- financial-identity-service -> identity crate, records on the store: the KYC profile a Book keeps for a person.
  A profile names a SUBJECT - the person it describes - which is not the caller: the caller owns the profile and the
  subject is recorded beside it, so a Book can hold profiles for people who never sign in. A profile arrives pending,
  with no documents and a risk of nothing. A verification with two documents or more marks the profile verified,
  stamps the moment and scores it twenty, low risk; fewer than two documents leaves the STATE where it was - it does
  not fail it - but scores the profile eighty, high risk, and the documents handed over are kept either way, which is
  what the shipped service did. A profile asked for by id, or by subject, is the caller's own or missing; by subject
  the EARLIEST profile the Book holds for that person answers, so a later registration does not displace the first
  answer. The documents arrive as repeated query parameters (?documents=a&documents=b), which is how the shipped
  endpoint took a List[str]; that exposed a defect in the shared query reader, which kept only the last value of a
  repeated key and would have silently dropped the rest - a repeated key is now read as a list. Routes:
  /profiles, /profiles/by-subject/{user}, /profiles/{id} and /profiles/{id}/verify. The service has its own domain
  crate, rust/crates/domains/identity, rather than sitting under platform with the caches and event buses: a KYC
  register is a business record set, not infrastructure.

- regulatory-compliance-service -> compliance crate, records on the store: the regulations a company is held to.
  A regulation arrives `pending_review` unless the caller names a state, and keeps the risk it is given, with a risk
  that is not named defaulting to medium. A listing is the caller's own regulations for one company, narrowed by
  framework when one is asked for and unfiltered when the framework asked for is empty. A status change refuses
  nothing: a state outside the four (compliant, non_compliant, pending_review, n/a) is ignored and the regulation
  keeps the state it had, a known state is taken, and either way the review date is stamped; a regulation that is
  not the caller's is missing. The dashboard's rate is the compliant share of the total, but a company with NO
  regulations scores a full hundred rather than nothing - kept because it is what the board has always said - and
  the critical list is every regulation carrying a high or critical risk that is not compliant. Routes:
  /regulations, /regulations/{company} for the listing and /regulation-dashboard for the board, because a bare
  /dashboard would not say whose figures they are.

- audit-compliance-service -> audit crate: the trail, its versions, the lineage it traces and the readings drawn
  from it. An event's checksum is taken over its body with the checksum left out, by one canonical serializer, so a
  stored event always verifies against itself; the shipped service hashed one rendering and verified another and
  therefore never matched, so only the sounding behaviour is carried over. The checksum covers the body only, not
  the checksum or the chain position. Each event takes the next chain position, which is what makes the trail
  append-only, and verification walks it in that order reporting a mismatch per event before folding the checksums
  into one chain hash. An actor is not the caller, so an event's user_id is kept under its own property and
  presented back as user_id, with every filter, report and search reading that one place. A version snapshot is
  numbered one after the last the caller holds for that RESOURCE ID - the shipped count ignored the resource type,
  so two types sharing an id share a numbering - and its checksum covers the state alone. Lineage is derived from
  the caller's events for one resource id: the first `depth` become nodes and an edge is drawn from a correlation
  id to the event it names when that correlation is one of the caller's events; a resource with no events answers
  "No lineage data found" rather than an empty graph. The planning, formal report, trail analysis and the
  monitoring, SOX and GDPR assessments are pure (entry points `plan_audit`, `generate_audit_report`,
  `analyse_trails`, `monitor_compliance`, `assess_sox`, `assess_gdpr`, named so no name is defined twice) and answered from the caller's figures: the plan works from a fixed
  million base rather than the company's own numbers (planning materiality 500,000, performance 375,000, thresholds
  18,750) and only calls the audit high at more than three high findings, medium at more than two medium ones, with a
  finding naming no level counted as medium; an opinion not named defaults to unqualified; a compliance rate with no
  checks is zero rather than a division; an untested SOX control counts as passing, which is what the shipped
  `tested: true` default did; and a GDPR assessment costs fifteen points a violation with a half-point floor that
  two violations never reach. The compliance report and the analytics summary are derived from the trail and never
  stored, with top resources and actors taken ten at a time. A search reads the text of the fields it is given, a
  structured field by the text it renders to, and a search with no query is refused rather than answered with
  everything. Where the shipped service stamped the clock, the port takes the caller's moment (verified_at,
  generated_at, monitoring_date, changed_at, the report date), so a reckoning can be repeated. Routes keep the
  shipped shapes, all currently free: /events, /versions/{type}/{id}, /lineage/{id}, /integrity/verify,
  /planning/plan, /reports/generate, /trails/analyze, /compliance/{monitor,sox,gdpr,report}, /analytics/summary,
  /search.

- audit-engagement-service -> audit crate, records on the store: an engagement opens `planned` with no findings, no
  summary and the caller's date as its start, and its four states are planned, in progress, completed and
  cancelled; a row for any other state or any other audit family is refused rather than stored, because the
  shipped enum rejected it; a summary only takes when the engagement is COMPLETED - one offered while moving to
  any other state is ignored and the engagement keeps the summary it had, as shipped, and completing stamps the
  end date the caller states; a finding defaults to medium severity, is raised against an engagement that must be
  the caller's, and the answer is just the engagement, the finding and its severity rather than the whole record;
  a finding is remediated by its own id, found among the caller's engagements, and one that is not the caller's is
  missing; the report counts critical, high, medium and low findings but NOT informative ones, while its total
  counts every finding, so the four can add up to less than the total (pinned by a test). Findings travel with
  their engagement as a property rather than as separate nodes, because a finding is only ever read back with its
  engagement. Routes: GET /engagement-report/{id}, because /report/{company_id} already answers the integrity
  report and axum will not register the same one-segment shape twice.

- audit-sampling-service -> audit crate, no records: a sample is answered and forgotten, which is what the shipped
  service did with it. The confidence factor is 1.96 at ninety-five per cent or above and 1.645 below. An
  attribute sample sizes from the factor and the tolerance alone and is capped at the population. A value sample
  runs the shipped formula, which puts the error rates in the numerator and the factor and tolerance in the
  denominator (`pop * p(1-p) / (conf^2 * tol^2)`) - the inverse of the textbook sum - and is then clamped to at
  least thirty and at most the population; on realistic figures that clamp nearly always saturates the sample at
  the whole population, and that is the answer shipped. A stratified draw splits by risk band (seventy and above
  high, forty and above medium, the rest low), takes half for high, thirty-five per cent for medium and fifteen
  per cent for low, but never fewer items than the band actually holds, so a small band is taken whole and the
  sample can exceed its nominal size; it is not random, it takes the band in order. A plain draw is random from the
  stated seed and the same seed draws the same items. A zero tolerance is refused rather than divided by, and an
  empty population answers a one-item interval and no sample. The conclusion multiplies the stated confidence by a
  hundred, so a ninety-five per cent sample states "9500% confidence" - the shipped answer, pinned rather than
  quietly corrected.

- sox-compliance-service -> compliance crate, records on the store: a control is registered active at medium
  risk and keeps the reference the caller gave it (SOX-ITGC-001) beside its own id; a test is only recorded
  against a control that exists and is the caller's, and the verdict is DERIVED rather than taken from the caller
  - no exceptions is a pass, fewer than a tenth in exception is a pass with exception, a tenth or more is a
  failure (the shipped comparison was `exceptions < sample * 0.1`, so exactly a tenth fails); a failing test
  raises its own deficiency before anyone asks, significant when more than a fifth of the sample failed and an
  ordinary control deficiency otherwise, plan left `TBD`; a deficiency states one of three severities and any
  other word is refused; marking it remediated stamps the caller's date; the dashboard counts a material weakness
  as open until remediated and does not count a remediated one at all. Routes: /sox-dashboard, because a bare
  /dashboard would not say whose figures they are.

- risk-assessment-service -> compliance crate, records on the store: a risk scores likelihood times impact and
  the score picks its level (up to 4 low, up to 9 moderate, up to 16 high, above that critical); it starts
  `identified` and is created returning only the id, score and level as shipped; the level is recomputed on every
  reassessment, so a mitigated risk that is reassessed downward stops being reported at its old level; an update
  that names no mitigation or status keeps the ones it had; the listing counts every level whether or not a risk
  sits at it, because the caller draws its bars from that; the dashboard answers zeroes rather than nothing for a
  company with no risks and names the five highest scoring risks; closing a risk keeps its score, level and
  mitigation for the record. Routes: the shipped service answered GET /risks/{company_id} and PUT/DELETE
  /risks/{risk_id} - the same one-segment shape, so the port answers all three methods on /risks/{id}, and the
  dashboard is /risk-dashboard/{company}.

- report-automation-service -> reporting crate, records on the store: a template states one of four report types
  (financial_summary, kpi_dashboard, compliance, operational) and anything else is refused rather than stored; a
  schedule must name a template that exists, so it can never be a schedule for nothing; a schedule starts active
  with its next run at the moment it was recorded until something runs it; a report runs only from an active
  schedule and running it is what moves the schedule's last run; a run copies the template's sections into the
  report and addresses it to the schedule's recipients, so a schedule addressing somebody reports itself
  `delivered` and one addressing nobody reports itself `generated`; a period is what the caller stated and the
  month it was recorded in otherwise; pausing and resuming are status changes, so the reports already produced
  stay readable; the reports listing is the newest kept, narrowed to one schedule. Route note: the produced
  reports are answered at `/generated-reports` because plain `/reports` already answers the receivables control
  account.

- report-distribution-service -> reporting crate, records on the store: a distribution list names its recipients
  and channels and defaults to email; a subscription must name a list that exists and runs daily, weekly, monthly
  or quarterly, with its next run at the moment it was recorded; the shipped endpoint took its format from a
  parameter called `fmt`, and the port accepts that spelling (and the plainer one); distributing is refused for an
  inactive subscription rather than quietly doing nothing; a distribution goes over the list's first channel and
  falls back to email when the list names no channel; the log keeps the recipients it went to, so the trail does
  not depend on the list still existing; the log listing is the newest kept, narrowed to one subscription.

- reporting-service -> reporting crate, shapes only: a report asked for answers with an id stamped from the
  moment it was asked for, the type it named, the word `generated`, and the recipients it was told about; a
  schedule answers active with its next run at the moment it was recorded. It generates no document and stores no
  report, which is how it shipped, so the answer carries `placeholder: true` rather than leaving a reader to
  discover it. The file address is the one the shipped service returned, `reports.example.com`, kept as shipped.
  The moment comes from the request rather than a clock, so the same ask answers the same id.

- dashboard-service -> reporting crate, shapes only: the four fixed layouts (executive, financial, operations,
  compliance) and the custom layout a caller states. Every figure on the fixed layouts is nil, because the
  shipped service read nothing to fill them, and the answer says so. An unknown dashboard type falls back to the
  executive layout as shipped; a custom widget is kept exactly as it was given, including the position the caller
  stated, and given a short id and position only where it stated neither.

- bank-reconciliation-service -> banking crate, records on the store: a statement imported with its own lines,
  the cash book it is reconciled against, the automatic matching of one to the other, the adjustment journal, and
  the outstanding items a reconciliation leaves behind. A statement's credit and debit totals are measured from
  its own lines rather than taken from the caller, and every line is stamped with the statement it arrived on; a
  cash book entry is recorded unmatched, because the shipped service refused to take `matched`, its
  reconciliation or the statement line it was matched to from the caller. A reconciliation subtracts bank errors
  from both sides, so a bank error moves the difference by nothing, and the two figures nobody supplies
  (unpresented cheques, uncredited deposits) are always nil; matching needs the same reference and an amount
  within a cent, so a line with no reference never matches, and it searches every cash book entry the caller has
  rather than only those on the account. Reconciliation records live on their own label: the shipped service
  shared `BankReconciliation` with cashbook-service, so the two shapes could read each other's rows.

- currency-service -> banking crate, records on the store: the currencies a Book deals in, the rates between
  them and the conversions over them. Fifteen currencies and their USD-relative rates are seeded per Book the
  first time it is touched, and only when the Book holds no currency at all, which is why a Book that deleted
  them all gets them back. A rate is a series rather than a value: the newest stated rate for a pair is the one
  a conversion uses, and older ones stay as history; an unstated pair is answered by its inverse and failing
  that by a cross rate through USD, so EUR to GBP is answered though nobody stated it. A converted amount is
  rounded half up to the target currency's own decimal places, so a yen conversion lands on a whole yen, and the
  arithmetic is exact decimals rather than the shipped floats, so a large amount keeps every digit. Kept as
  shipped: the balance-style alert is answered and never stored, `PUT /rates/{from}/{to}` restates the newest
  row for the pair instead of adding a second, and the statistics count the rates as rows rather than as pairs.

- bank-feed-service -> banking crate, records on the store: the connections a Book holds, the transactions that
  arrive from them, the syncs over them and the rules that pick those transactions out, plus MT940 parsing,
  provider webhooks and statistics. A transaction may only be imported against a connection the caller owns;
  the same external id on one connection is a duplicate and answers 409, while the same id on two connections is
  two transactions, which is what the shipped duplicate check compared; an imported transaction starts `cleared`
  or `pending` and never takes a status from the caller. A rule matches when every condition it states holds,
  the highest priority matching rule wins, an asleep rule never matches, and an amount band is measured against
  the absolute amount so a debit of 500 sits inside 100 to 1000. MT940 is parsed only for the tags it understood
  - `:86:` is what closes a transaction, so a statement without one yields nothing - but the amount reader was
  repaired: the shipped one swallowed the bank reference into the number and raised on most real statement
  lines, so the amount is now read as the decimal that follows the debit/credit mark. The balance answer is the
  fixed placeholder the shipped service returned while the provider calls were a stub, and it says so.

- bank-relationship-service -> banking crate, records on the store: the banks a company deals with, the quality
  of the service it gets from them, and the summary of the whole relationship book. A relationship starts
  `active` and rated 3 when the caller states neither, a rating is held to 1 through 5, and a status has to be
  one of active, dormant, terminated or prospective. An update moves only the rating, the status and the notes,
  and answers those three fields rather than the record, as shipped. A quality metric may only be pinned to a
  relationship the caller holds, so it cannot be recorded against somebody else's bank; a relationship's average
  is taken over its metrics oldest first, and one with no metrics averages nil rather than dividing by nothing.
  The summary counts distinct bank names and distinct services, not rows.

- banking-integration-service -> banking crate, records on the store: the plain connection to an account, the
  sync over it, what arrived, and the reconciliation of it. One connection per bank and account number, refused
  as a duplicate otherwise; a sync is refused on a connection that is not active and otherwise stamps the moment
  it ran; a transaction is only reconciled through the connection it belongs to, so naming another connection's
  transaction is a miss; disconnecting marks the connection disconnected and keeps the record. One record shared
  deliberately: this service wrote `BankConnection` under its own edge and its transactions under another label,
  so the feed's door and this one could not see each other's rows. Both doors now write one `BankConnection` on
  one edge and one set of bank-feed transactions, so a transaction reconciled here is reconciled for the feed
  too, and each door answers the fields its own shape needs with defaults rather than inventing the others.

- cash-management-service -> treasury crate, records on the store: the cash accounts money sits in, the
  transfers between them, and the liquidity those balances add up to. A transfer can only move money between
  the caller's own Book-visible accounts, and only if the source keeps its stated minimum; it is recorded as
  completed, both balances move together, and a transfer from an account to itself nets to nothing. Liquidity
  is the operating, reserve and investment balances added together, with the ratio over short-term
  obligations rounded to two places and published as a plain figure rather than a string, because a ratio is
  not an amount of money; with nothing owed it reports no ratio instead of dividing. One record shared
  deliberately: this service and cash-optimization-service both wrote `:CashAccount`, each under its own
  edge, so neither could see the accounts the other made. The port keeps one `:CashAccount` under one edge,
  so the accounts the two services describe are one set, and liquidity counts an account whichever of the two
  shapes it carries.

- treasury-management-service -> treasury crate, records on the store: a company's dated cash movements, the
  position it reports, the burn that position is judged against and a straight-line forecast from the history
  it has. Shapes kept: a recorded movement answers with its id and the word `recorded` rather than the record; a
  company's movements are listed oldest first and the total counts every one of them while the list handed back
  is only the most recent `limit`; a stored position wins over a derived one, and with none stored the cash is
  simply the inflows less the outflows with liquidity left at adequate; liquidity is the months of cover the
  available cash gives over the last thirty days of outflow (nothing spent, or more than six months, is excess;
  then adequate, tight, critical); a forecast with no history is all zeroes and says so; the average is taken
  over the days since the first movement with a floor of one, and the projection always ends at the current
  available cash plus the net, restricted cash included; the four investment options are the fixed list the
  service offers, not something it prices. Two things are deliberate: the forecast and the burn are measured
  against a `now` the caller supplies, where the service read the server clock, so the same history always
  forecasts the same figures; and its movements are NOT merged with the `CashFlowEntry` label, which holds a
  project appraisal year rather than a dated movement, so one label would have made each other's rows
  meaningless.

  Found while porting, and fixed for every port that reads a query: a page size or a filter sent as text -
  which is how a query string arrives - silently kept its default, so `limit=5` answered with a hundred. One
  set of query readers in `vimbai_store::query` now reads numbers and flags from text or numbers, and cashbook,
  cash management and treasury management all use it instead of their own copies.

- treasury-policy-service -> treasury crate, records on the store: the policies a Book sets, the limits each
  one places, and the checks made against them. Shapes kept: a category outside the six the service knows is
  refused with the list of valid ones; a limit can only be set on a policy the caller can see, and a check only
  against such a limit, so a policy in another Book is a miss rather than a refusal that admits it exists;
  utilization is the checked value over the limit as a percentage, with a limit of zero reporting nothing rather
  than dividing; a check is compliant when the value is at most the limit, so landing exactly on the limit
  passes; the limit's own `current_utilization` is set to the checked VALUE rather than the percentage the check
  reports, which is what the service did, and both figures are kept because older callers read that field; and
  the compliance listing filters by policy and hands back the most recent `limit` checks in the order they were
  made.

- trade-finance-service -> finance crate, records on the store: the instruments a company trades on - letters of
  credit, documentary collections, bank guarantees, advance payments and factoring - and what each costs and risks.
  Shapes kept: the fee is a rate on the face amount by type, 0.2% for a letter of credit, 0.1% for a documentary
  collection, 1.5% for a bank guarantee, nothing for an advance payment and 3% for factoring; the risk band follows the
  face amount itself (under 100,000 low, under 500,000 medium, above that high); the documentation is the four papers
  every shipment needs, with a letter of credit adding its application and the proforma invoice; an instrument type
  outside the five is refused; presenting documents and settling only move the status; an instrument the company does
  not hold is a miss; the listing reads by issue date then id, oldest first.

- investment-monitoring-service -> finance crate, records on the store: the portfolios a company invests through, what
  each holds, and how it has done. Shapes kept: a holding's worth is quantity times price, exactly; performance is the
  worth of the holdings against what they cost, the gain as a percentage of the cost and nothing when the cost is
  nothing rather than dividing; a portfolio with no holdings cannot be measured and is a miss rather than a report of
  nothing; a holding is added only to a portfolio the caller holds; a price change answers with what the holding is now
  worth and leaves the measurements already taken alone; performance history reads newest first, thirty by default.

- personal-finance-service -> finance crate, records on the store: a household's recurring bills and income, the
  debts it is paying down, what it holds, its tax bands, goals and income sources, and the one-page view over all of
  it. The part that matters is the debt ledger: a payment does not simply reduce the balance, because interest is
  charged daily on whatever was outstanding since the previous payment and only the rest touches the principal, so the
  state of a debt is a replay of its payments and two people making the same payments at different intervals do not owe
  the same amount. Shapes kept: a recurring item is a bill or an income on one of four frequencies, and a due date that
  falls past the end of a shorter month is held at the 28th so February is never skipped; deactivating keeps the item
  rather than deleting it; a debt's scheduled payment is the level payment that clears the principal over its term, and
  nothing when the rate is nothing, in which case the principal is spread over the months; a payment smaller than the
  interest leaves the principal alone rather than making it negative; a projection whose payment cannot cover the
  interest is refused rather than run forever; a sale of more units than are held is refused, a buy re-averages the
  cost and a sale keeps it and reports what was realised; a person with no bands of their own is taxed on the four the
  service ships (nothing to 10,000, 15% to 30,000, 25% to 70,000 and 35% above), and a set of bands must ascend by
  upper limit, be ordered by rate and end without one; the average goal progress is nothing when there are no goals,
  and the debt-to-income ratio divides by one rather than nothing when there is no income.
  Correction of form: the original read the server clock for the day a run happened and for a projected payoff. The
  port takes that date from the request as `today`, so a reckoning can be repeated and a test can pin it.

- rolling-forecast-service -> budgeting crate, records on the store: a forecast that moves, with periods appended as
  time passes and actuals filled in behind it. The periods ride on the forecast rather than standing alone, so recording
  an actual rewrites the forecast. Shapes kept: a frequency outside weekly, monthly and quarterly is refused; the current
  period is the first period given and empty when there are none; an actual for a period the forecast does not carry
  changes nothing and reports no variance rather than adding the period; the variance is actual less forecast, and its
  percentage is against the forecast and nil rather than a division when the forecast is nil; rolling reports how many
  periods the forecast now holds.

- scenario-analysis-service -> budgeting crate, records on the store: three assumptions set side by side over one base of
  figures. `/analyze` is arithmetic over the payload and keeps nothing; the scenario records are the assumptions a
  company wants to remember and compare. Shapes kept: an unstated assumption keeps the shipped rates (revenue up 10%,
  costs up 5%, interest 5%, tax 25%); a loss carries no charge, since tax is on the positive part of pre-tax income only;
  the margin is against revenue and nil rather than a division when there is no revenue; the revenue sensitivity is the
  best case's net less the worst case's and the cost sensitivity the base case's net less the worst case's, which is the
  pair the service reported even though the labels invite a closer reading; the recommendation follows the worst case
  first, then the base case; a comparison needs two scenarios and reports the range between the best and the worst.

- sensitivity-analysis-service -> budgeting crate, records on the store: how much the target moves when one variable is
  nudged. Shapes kept: every variable is swept across every step, so the count is the variables times the steps; the
  change_s steps default to -10, -5, 0, 5 and 10; a base target below one is measured against one rather than itself.
  FINDING PINNED, NOT FIXED: the impact of a step is not measured but estimated from the variable's *name*, reading words
  like "revenue", "cost" or "interest" and applying a fixed share (six tenths for revenue against a profit target, four
  tenths the other way for cost, a tenth for interest, a fifth otherwise; seven and five tenths for cash flow; eight and
  nine tenths for price and volume against revenue), and the elasticity is the step divided by the target's relative
  move, the reciprocal of the usual definition. So "most sensitive variable" points at the variable that shifts the
  target *least*. Ported as it stands, because the answers are what callers see, and each point is pinned by a test.

- scenario-modeling-service -> budgeting crate, records on the store: scenarios built from labelled variables, rules that
  fire on values, and what-if runs against them. A rule holds conditions that must all hold and actions to take when they
  do; comparison is by number once both sides read as numbers and by text otherwise, which is how "contains" works. A
  scenario's outcome is read off by its type: a budget forecast adds its variables up, a revenue projection compounds a
  growth rate over the months between the two dates at the shipped thirty-day month, an expense simulation adds the
  variables whose names mention expenses, a cash flow nets inflows against outflows, a profitability subtracts costs from
  revenue. Shapes kept: a condition on a field the figures do not carry is not met rather than treated as zero; every
  rule fires in the order the scenario names them, with nothing sorted by the priority the rules carry; a what-if over a
  variable the scenario does not carry leaves it alone rather than adding it; the variance is the new outcome less the
  original and its percentage is nil when the original was nil; the affected accounts are the shipped split of the
  variance - four tenths revenue, three tenths expenses, three tenths net income, a shape rather than a posting; a swept
  variable offers at most five recommendations; a comparison names the best and worst of the scenarios given and reports
  nothing else, with the comparison date taken from the caller so the reckoning repeats.

- absorption-costing-statement-service -> costing crate, records on the store: the trading account that runs from
  sales to gross profit and, once expenses are added, to net profit; and the production cost statement that runs from
  materials through prime cost to the cost of each unit. Both are kept, not just computed, and the line items are
  built in the shipped order with the totals flagged. Adding expenses appends a profit section to the statement it
  finds and marks it completed, so a second call appends a second section; adding expenses to a statement the caller
  does not hold is a miss rather than a new statement. The cost per unit is left at nil rather than dividing when
  nothing was produced. NOTE the paths: this is *not* the accounting crate's trading-account-service, which books
  sales less returns, direct wages and factory overhead and reports a gross profit percentage. Two different services
  behind one familiar name, so the routes here sit under `/absorption-costing-statement/`.

- activity-based-budget-service -> budgeting crate, records on the store: activities carrying a cost pool, a driver and
  a rate, and budgets that price the driver volume expected in a period at those rates. A line naming an activity the
  caller does not hold is refused by name rather than dropped; a line's cost is the volume times the rate the activity
  carried when the budget was made, so a later rate change does not rewrite an agreed budget; the total is the sum of
  the lines and nothing else; approving sets the status and leaves the figures alone.

- partnership-agreement-service, partnership-changes-service, partnership-dissolution-service and partnership-revaluation-service
  -> partnerships crate, records on the store: one crate for the four services that
  answer questions about the same register, with their shared arithmetic (a partner's contribution and the capital a
  list of partners puts in) in one place rather than four, so an agreement and the summary that reports it cannot
  disagree. Agreements carry their partners and the capital they add up to; a partner added takes the agreement's
  capital with it, and a change touches only the fields it names. Changes record admission, retirement, death and
  settlement: an admission keeps the draft for the capital brought in and one for any goodwill paid, and skips the
  capital draft when nothing was brought; a retirement is owed capital, current account and goodwill, and its single
  draft debits both balances and credits cash; a death is owed the two balances with no goodwill and names the
  executor as its note. Shapes kept: settlement never checks the state it is coming from, so settling twice is
  allowed; a dissolution answers a report the caller does not hold as an error body rather than a miss, and so does a
  revaluation; a creditor's discount is what the caller says it was rather than what the amounts imply; on
  dissolution an asset sold at exactly book value is a decrease of nothing rather than a profit; a revaluation writes
  an asset up when its new value is over its old one and down otherwise, keeps the two totals apart, and raises a
  goodwill reserve only under `raise_and_raise`. No service here moves money: every draft is kept with the record and
  posting it stays the accounting books' business.

- npo-service -> the non-profit crate (rust/crates/domains/npo), records on the store: funds and their restrictions,
  the money moving through them, donations, grants and drawdowns, donors, projects, programs, budgets and their lines,
  internal controls, audit reports, program metrics, impact measurements, volunteer records, and the two statements.
  The whole non-profit domain is ONE crate: npo-scale-service joins this one rather than getting a crate of its own,
  so a fund balance is worked out one way whether a small charity or a multi-branch non-profit is asking. A fund
  balance is never taken from a caller: it starts on its opening balance and moves by the transactions recorded
  against it, with the running contribution and disbursement totals following the same split. A second fund with the
  same code in the same Book is refused rather than created. A permanently restricted or endowment fund accepts only
  investment and appreciation, so a contribution into one is refused exactly as a disbursement is. Net assets are read
  from the ACTIVE funds, general/operating/board designated outside the donor's restriction and everything else
  inside it, with the total being the funds' own total and the closing figure standing in for the opening one because
  the service had no earlier snapshot to compare against. A statement of activities counts donations in the period,
  grant money received, and fund disbursements in the period, and reports the difference as the change in net assets.
  A statement of financial position sets the funds' total against no liabilities, which is what the service shipped.
  SROI analyses and NPO assets are answered WITHOUT being kept, as they were before - neither had a store - and an
  asset's net book value is its acquisition cost with no depreciation. Drawdowns raise the grant's amount received;
  budget lines and fund restrictions refuse to hang off a budget or fund the caller does not hold.

- family-community-group-service -> partnerships crate, records on the store: savings groups, the members in them, the
  contributions each makes and the payouts read back. A group's four figures belong to the register, not the caller: a
  new group starts at cycle one, with a pool of nothing, no members and active, whatever the request claimed. The
  frequency is one of weekly, biweekly, monthly and anything else is refused by name before a record is written. A
  member who brings no amount of their own takes the group's, and the group's member count is *recounted* from the
  members it holds rather than incremented, so adding twice cannot inflate it. A contribution is accepted only from a
  member the group actually holds, carries the cycle the group is on at the time rather than one the caller claims,
  and raises the pool by exactly the amount paid in - worked out from the record, so the pool and the contributions
  listed against the group cannot tell different stories. Advancing a cycle moves it on by one and answers where it
  landed. Everything is held through the store, scoped to the caller's Book; a group the caller does not hold is a
  miss rather than a leak.

- double-entry-principles-service, control-account-reconciliation-service, financial-integrity-service and financial-state-machine-service
  -> the ledger kernel's services crate (rust/crates/kernel/ledger-services),
  beside ledger-core rather than in a domain of their own, because all four answer questions about the same journal.
  ledger-core stays the sole authority for posting integrity; these are the kernel's HTTP and record surface.
  Double entry is the reference and stores nothing: the sixteen account types, which side each grows on, the seven
  journal templates, and the suggested entry for a sale, a purchase or a depreciation. Control account keeps the
  breaks - a variance is always the absolute difference, a resolved break stops counting as money at risk, and a
  reconciliation snapshots the pending breaks it found rather than linking them, so a later reconciliation does not
  rewrite what an earlier report saw. Financial integrity is the balance, hash and completeness checks and the
  report over one company's checks; a company with nothing recorded reads nil rather than dividing by nothing.
  The state machine walks a document draft -> pending approval -> approved -> posted -> archived, checks every move
  against the map before applying it, appends each move to the document's history, and creates every document in
  draft whatever state the caller sends, because a document that arrives already posted has skipped the approvals
  the map exists to enforce. Two answers differ from the Python on purpose: `/validate-journal-entry` accepted a
  one cent drift and now refuses anything that is not exactly balanced, and `/account-rules/{account_type}` raised
  for every account type because its examples catalogue named two members the enum never had, so the port answers
  the rules and examples it meant to give. One boundary is named rather than crossed: ledger-core validates balance
  in f64 with a 1e-9 tolerance, which would put floats back into a posting decision, so this service answers in
  exact Money and does not call it - moving that validator onto Money belongs in ledger-core. The documents carry
  their own `OWNS_FINANCIAL_DOCUMENT` edge rather than borrowing the accounting crate's document edge, so the two
  registers cannot be mistaken for one.

- revaluation-model-service -> assets crate, no records: a revaluation pass plus the four questions that follow one.
  One pass decides per asset whether fair value sits above or below its carrying amount; gains go to a revaluation
  surplus in other comprehensive income, losses go against that surplus first and only the rest to profit or loss, and
  the surplus is a running figure in the order the assets are listed, so a loss only finds a surplus an earlier gain in
  the same list created. An asset with no fair value given is measured at cost, which for a depreciated asset reports a
  gain. Shapes kept: depreciation is adjusted only for assets written *down*, which is what the service did; the
  remaining life counts from a fixed two years held rather than from dates; and a rate over nothing reports nil where
  the service would have failed the request. The four companions answer the surplus after deferred tax, the
  depreciation over the remaining life, an impairment against a surplus standing, and the indexation allowance on a
  disposal. The frequency-check and regularity fields carry no arithmetic, as they did not before. These five answer
  from their figures alone and touch no records, so they need no Book.

- budget-service -> budgeting crate: the master budget, what it is doing against actual, and the projections drawn
  from it. Variance is always actual less budget, so a negative variance is money not spent, which for a cost line is
  favourable. Shapes kept: an account that carries no actual is measured at 95% of its budget rather than at nil; a
  variance against a nil budget is nil rather than a division; a budget is on track while it is inside five percent
  either way and needs attention outside that, and a line is lifted out as significant past ten; the detailed analysis
  reports the net as the unfavourable side less the favourable one, so an underspend reads negative; a forecast starts
  from the sum of the history, grows by 3% unless told otherwise, is multiplied by the quarter's seasonal factor and
  carries a confidence band of a tenth either way; the rolling forecast's base figures are the shipped placeholders
  (1,000,000 revenue, 700,000 expenses, 300,000 profit growing at 5%, 3% and 7%), which is a shape rather than a model,
  and its arms bend revenue and expenses in opposite directions. Correction of form: the rolling forecast's id is built
  from a `today` supplied by the caller instead of the server clock.

- zero-based-budgeting-service -> budgeting crate, records on the store: budget packages built up from nothing, each
  line argued for on its own. A package holds its items and every question about it is answered by rewriting the
  package, so the items are part of the package rather than records of their own, which is how the service kept them.
  Shapes kept: a package starts as a draft and its total is the sum of its items as given; adding an item recalculates
  the total rather than adding to it, so a stale figure cannot survive; a priority outside one to five is refused and a
  priority change may carry a status with it; a status moves only among draft, submitted, reviewed, approved and
  rejected; the summary counts packages, totals them and groups them by department and by status, answering with zeros
  when there are none.

- net-present-value-service -> finance crate, no records: what a project's future flows are worth today once the
  investment is paid for, with the rate given as a decimal or as a percentage. Discounting is exact, one year at a
  time, through the one engine this crate's four capital-budgeting services share, so a change to how a discount
  factor is worked out reaches all of them. Shapes kept: the decision is accept for a positive net, reject for a
  negative one and indifferent at exactly zero; a residual value belongs to the final year and is discounted the same
  number of years as the last flow; the schedule publishes the discount factor to four places and the present value to
  two; a project in a comparison with no flows or a zero rate is left out of the ranking rather than ranked last, and
  the recommendation is nothing when no project was usable; the profitability index is the inflows over the
  investment, and an investment of nothing indexes zero rather than dividing.

- internal-rate-return-service -> finance crate, no records: the rate at which a project's flows are worth exactly what
  it costs. The search is the only place in the crate that iterates rather than computes, because a root is an
  approximation by nature; what it settles on is then priced exactly and that exact net is what the answer reports.
  Shapes kept: an empty series or an investment of nothing is refused rather than guessed at; the search starts from
  the average flow over the investment; a rate below -0.99 resets to -0.5 and one above ten to five, so a wild step
  cannot run away; the percentage is the decimal times a hundred only when the decimal is positive, which is how the
  original published a negative rate. Correction: the original never checked the rate it found against the net it
  produced, and a project paying back 1100 for 1000 inside one year, which is ten percent, came back as -0.81. The
  rate is now verified against the exact net and re-found by halving a bracket when it does not hold.

- payback-period-service -> finance crate, no records: how long a project takes to give back what it cost. Shapes kept:
  a level series is one division and a series that moves is walked year by year with the part-year interpolated rather
  than rounded up; a level flow of nothing or less is refused rather than divided by; a project that never covers its
  investment reports no period at all and says so rather than giving a figure; the period is published to two places
  and also as a sentence of years and months; a comparison ranks the quickest first and leaves out any project that
  never pays back; the average-flow route spreads the total over the years and then divides the investment by that
  average.

- continue-shutdown-decision-service -> finance crate, records on the store: whether a department, product or division
  carries on trading. `/analyze` takes the whole picture and keeps the decision; `/quick-analysis` takes the two
  figures that settle it and keeps nothing. Shapes kept: contribution is revenue less variable costs; continuing
  leaves the contribution less the costs a shutdown cannot avoid; stopping leaves only those unavoidable costs, less
  what it saves, so the avoidable costs come back, the shutdown costs are paid and any other saving is added; the
  recommendation is whichever leaves more, and indifferent when they are equal; the quick analysis says "CONSIDER
  SHUTDOWN" rather than "SHUTDOWN" because two figures alone cannot settle it; a decision the caller does not hold
  answers with the shipped `{"error": ...}` body rather than a miss. Not carried: an HTTP client for accounting-service
  that no endpoint ever called.

- foreign-exchange-service -> treasury crate, no records: the spot table, a conversion between two currencies,
  and what an exposure costs when it is only partly hedged. Nothing is stored, every answer is worked out from
  the request and the table. Shapes kept: the table is the service's own, base USD, and rates are exact decimal
  text rather than floats so a large amount does not drift in the last cent; a currency outside the table is a
  miss on `GET /rates/{currency}` and a refusal on convert and hedge, which is how the original answered; a
  conversion crosses through the base (100 EUR into ZAR is 100 / 0.92 * 18.5), the rate answers to six places and
  the amounts to two; a hedge with no forward rate falls back to spot and its potential loss is what the unhedged
  part would move by per hundred units of spot, which is the figure the original published; the hedge cost is
  0.2% of the hedged part; a hedge ratio above one would cover money that is not exposed and is refused. The
  conversion no longer mints an id: a conversion is not a record.

- petty-cash-service -> treasury crate, records on the store: funds, the spending out of them, the vouchers that
  authorise it and the replenishments that put the money back. Shapes kept: the same four transaction groups the
  service knew (fund status, transaction type, payment category, replenishment status) refuse anything outside
  them with the list; a receipt, a replenishment and the initial fund all count as money in and only a payment as
  money out, so the balance is worked out from the transactions every time it is asked for rather than stored
  beside them, which is what keeps the two from disagreeing; a voucher brings its payment with it as one act, so
  the balance moves the moment the voucher exists; a payment for more than the fund holds is refused rather than
  driven negative; a fund is closed by a status change, not a delete, so its history stays readable; a minimum
  above the maximum is refused. One correction: the original counted outstanding vouchers as transactions whose
  status was pending, and a transaction has no status, so it always reported zero; what is actually outstanding
  is a voucher still waiting, so that is what is counted. One label shared: this service and accounting's petty
  cash book both wrote `:PettyCashFund`, each under its own edge, so neither could see the other's funds; the port
  keeps one label under one edge, and the funds both describe are one set.

- treasury-risk-service -> treasury crate, records on the store: the exposures a Book carries, the value at
  risk calculated over a portfolio, the stress scenarios and the results of running them. Shapes kept: an
  exposure type outside the five the service knows is refused with the list; a scenario shock type outside its
  six is likewise; value at risk is the portfolio times its daily volatility times a z-score the service carried
  per confidence (1.282, 1.645, 2.326) times the square root of the holding period, rounded to the cent with the
  percentage to four places, and a portfolio of nothing reports no percentage rather than dividing; a scenario
  the caller cannot see is a miss rather than a refusal; a market crash or an fx devaluation takes its
  percentage off the portfolio, a rate shock moves basis points against an assumed five-year duration, and any
  other shock type costs nothing and says so; the dashboard adds the notionals by type and counts what is
  stored. The one approximation left is the square root of the holding period, which has no exact decimal form;
  everything else on the path is exact.

- treasury-reporting-service -> treasury crate, records on the store: the reports it generated and the figures
  they carry. Three generators and a listing. Shapes kept: a cash position totals the usd balances and groups
  the native balances by currency, so the two figures answer different questions and neither is a conversion of
  the other; an fx report totals the exposure and the unhedged amount and averages the hedge ratio over the pairs
  it was given, with no pairs reporting no average rather than dividing by nothing; the debt report publishes the
  totals and rate it was handed and counts the instruments; the listing narrows by report type and hands back the
  most recent reports in the order they were generated. A report in another Book is a miss, not a refusal that
  admits it exists. The summary figures are decimal strings throughout, so a report of a twenty-digit balance
  keeps every digit.

- sovereign-treasury-service -> treasury crate, records on the store: a country's accounts, the debt it carries,
  and one fiscal position a year. Shapes kept: an account type outside the four the service knows is refused with
  the list, and a created account answers with only the id, the type and the balance a desk reads, not the whole
  record; debt answers with the instrument and the outstanding figure; the fiscal position is one per country, a
  second one replaces the first rather than piling up, the deficit is derived as expenditure less revenue so a
  surplus comes out negative without a special case, and a ratio nobody supplied stays zero rather than being
  invented; a position the caller does not hold is a miss, not a refusal that admits it exists.

- treasury-compliance-service -> treasury crate, records on the store: the regulation checks a treasury runs and
  the verdict recorded against each. Shapes kept: the first read of a company's checks seeds the six defaults and
  keeps them, so the second read finds them rather than seeding again, and a seeded check starts awaiting review;
  the report never seeds, because a report with no checks should say `Run compliance checks first` rather than
  quietly creating them, and it reports a rate of one with nothing to fail; a rate is compliant over every check,
  including ones nobody has reviewed yet; a finding is a warning or a not-compliant verdict, and the
  recommendations are the remediation notes of those findings, so an un-noted finding contributes nothing; a check
  in another Book is a miss rather than a refusal that admits it exists. Deliberately its own label rather than
  the `ComplianceCheck` the accounting standards service uses: that records whether the books follow a standard,
  and one label would let a standards verdict land in a treasury compliance rate.

- debt-management-service -> treasury crate, records on the store: the loans a company carries, the schedule that
  pays each one down, and the summary the book adds up to. Arithmetic is the service's own in decimals: a monthly
  payment is the annuity formula with the rate over twelve, and a zero rate simply divides the principal over the
  term rather than dividing by nothing; the schedule charges interest on what is still owed, puts the rest of the
  payment against the principal, and reports a balance of nothing rather than a negative from the last penny; the
  summary weights the rate by balance rather than by loan, publishes the rate in the decimal it works in, and
  reports no ratio rather than dividing when there is no debt or no equity. The payment is computed once, here;
  the debt portfolio report takes its weighted rate as given rather than recomputing it.

- sales-ledger-control-service, bad-debts-recovery-service, provision-bad-debts-service and provision-doubtful-debts-service
  -> receivables crate, records on the store: one crate for the four services that measure what will not be collected,
  sharing one exact percentage helper and one calendar-day counter rather than four copies of each. The sales ledger
  is its transactions: a debtor's balance is replayed from them in creation order, so the balance cannot drift from
  the entries behind it, and the control account is the movement totals whose closing figure is the same number the
  replay reaches - reconciling the two is a comparison, and anything under a cent counts as agreed. A type that
  neither adds nor takes away is accepted and moves nothing; an as-at date keeps the days up to it, read as calendar
  days rather than by clock time; a debtor balance reports that debtor's last ten entries. Bad debts carry what was
  written off and what has come back: a recovery leaves its own journal beside the debt, a debt is completed once
  what came back reaches what was written off, a completed debt is refused rather than banked twice, and the summary
  counts only incomplete debts as still owed, with a rate of nil rather than a division when nothing was written off.
  The aged provision starts from the debtor's stored days and adds the days since the period end, so the same period
  run twice ages twice - the shipped shape, pinned by a test - and the journal never threads a policy through, so it
  always ages against the default ladder, crediting the size of a negative adjustment. The graded provision escalates
  and never relaxes: a debtor graded doubtful stays doubtful however current it is, an assessment by the caller may
  lower a grade, an unknown grade earns the medium rate, days overdue count from the due date and never go below nil,
  and a provision sums the entries it is given rather than recalculating them. Two defects were found and fixed on the
  way in: the shared percentage helper multiplied by the rate instead of dividing by a hundred, so a 25% provision was
  fifty times too large, and the day count ran backwards, so every debtor aged to zero days and no provision was
  raised. Both are pinned by tests.

- tax-accounting-service -> the tax crate (rust/crates/domains/taxation), records on the store: rates and their
  registration, the tax worked out on a transaction (single or as a batch), returns with their filing and payment,
  withholding tax, the summary and VAT-by-jurisdiction reports, deferred tax, and the liability schedule. One crate
  for the tax domain, so the three tax services answer a rate question one way. Which rate applies is decided once
  (crates/domains/taxation/src/rates.rs): a registered rate covers a window from `effective_from` to `effective_to`, the
  first that covers the day wins rather than the newest, and a tax type with no registered rate falls back to the
  shipped default table - so a VAT sale answers 20% before any rate was ever entered rather than being refused. The
  net/gross split is applied once there too: a net amount has the rate added on, a gross amount has it divided back
  out, and only the tax is rounded to the cent so the two figures always add back to the gross exactly. A return is
  never taken from the caller: its sales, purchases, collected and paid figures are measured from the transactions in
  its own period and its net due is collected less paid plus the adjustment stated, with a reversal excluded because
  the entry it reverses is already in the period. The VAT breakdown puts everything that is not a sale on the paid
  side, which is what shipped - imports and expenses included - and the liability schedule counts only a filed return
  as outstanding, since a draft is not yet owed. Nothing here moves money: a calculation stores the transaction and
  reports the journal it would post, and filing or paying a return writes the status and the date, exactly as the
  shipped service said in its own comments.

- tax-compliance-service -> the same tax crate (rust/crates/domains/taxation, module `compliance`), obligations on the
  store: created pending whatever the caller says, listed by due date with the status filter applied to the answer
  rather than in the query, filed with the figure stated or with the obligation's own amount when the caller names
  none, and summarised as counts, a score and the deadlines inside a month. Filing another company's obligation
  answers exactly as a nonexistent one does, so the register cannot be used to discover another tenant's work. A
  company with nothing owed scores 100 rather than nil, and an already overdue deadline is listed with a negative
  count of days because it is the most urgent thing on the list. The day the summary measures from is the caller's
  `today`, not the clock, so the same obligations answer the same summary twice. The route is `/compliance-summary`
  because `/summary` already answers the receivables control account.
- tax-planning-service -> the same tax crate (module `planning`), strategies on the store and `/plan` answered from the
  request: savings larger than the bill plan a nil bill and never a refund, a strategy that costs nothing states no
  return (`null`, not an infinite one) and is never advised, and only a low or medium risk strategy returning more
  than it costs is recommended - a high-risk saving is measured but never advised. A plan carries an identifier even
  though nothing is stored, so a caller can refer to the answer they were given.

- purchases-ledger-control-service and expense-tracking-service -> payables crate
  (rust/crates/domains/payables), records on the store: one crate for the two services that answer the same money
  going out. The purchases ledger is its transactions: a creditor's balance is replayed from them in creation
  order, so the balance cannot drift from the entries behind it, and the control account is the movement totals
  whose closing figure is the same number the replay reaches - reconciling the two is a comparison, and anything
  under a cent counts as agreed. A discount is recorded and moves nothing, because the shipped service listed it as
  a type the API accepts but left it out of both sign sets; an as-at date keeps the days up to it, read as calendar
  days rather than by clock time. Expenses are separate records rather than a ledger: a claim arrives pending
  whatever the caller sent, because the status, the approver and the rejection reason belong to the service; an
  amount of nothing or less is refused, and so is a category outside the nine the service accepted; a page larger
  than the service's own largest is refused rather than quietly filled; approval records who approved and rejection
  records why, each answering only the three fields the shipped route answered; and the summary measures the same
  listing, so it counts at most the listing's own page of claims - the shipped shape, pinned rather than smoothed
  over. A defect was found on the way in that also affected the receivables port already on main: an entry type that
  moves nothing still added its amount to an existing balance, because the replay treated a zero direction as "add"
  for anyone already in the list. Rather than fix it twice, both ledgers now call one shared replay in
  `vimbai_common::ledger`, which has its own tests for the zero-movement case. The plain `/transactions`,
  `/reconcile` and `/control-account/summary` already answer the sales ledger, so the purchases routes are named
  `/creditor-transactions`, `/purchases-reconcile` and `/purchases-control-account/summary`.

## Crate already has the maths, service not yet wired

These are the cheapest ports left: the crate already holds the arithmetic, so the work is a
module that shapes the request and answer and a route in front of it. There are five, not the
forty-five this section used to list - the other forty named services whose directories were
deleted when their maths moved, and a list of services that no longer exist is not a backlog, it
is a list of things to go and check. `tests/test_port_tracker.py` now reads this file and fails if
an entry names a service that is not on disk.

- budget-service -> budgeting::prepare_budget, budgeting::monitor_budget
- payback-period-service -> capital_valuation::payback_period
- present-value-service -> capital_valuation::present_value, capital_valuation::present_value_percent
- reporting-service -> advisory::analyze_events_after_reporting
- rolling-forecast-service -> budgeting::rolling_forecast
## Needs the maths written in Rust

- npo-scale-service, all but its Book plumbing (same crate as npo-service, rust/crates/domains/npo): every
  register and report answers from Rust now - organisations created at the band their own revenue earns, the branches
  and people that push that band up, feature sets and approval limits, branches, vendors, revenues, purchases that
  settle once and book the expense they became, donors, donations, pledges with the scheduled-giving collector,
  numbered receipts with public token verification and their PDF, expenses with the band-aware dual approval, budgets
  that replace a line rather than adding a second, compliance items, balance items, shareholders, directors, dividends
  capped at what the company has earned, partners with draws and capital accounts, and the five reports (activities,
  functional expenses, position, budget vs actual, consolidated) plus the creditors and equity ones. An organisation
  is the caller's alone: another tenant's answers exactly as a nonexistent one, so the registry cannot be used to
  discover it. THE SERVICE STAYS ON THE PYTHON ENGINE for its four Book routes (`GET /orgs/{id}/book`,
  `POST /orgs/{id}/book/link`, and `GET`/`POST /orgs/{id}/members`), which proxy the platform's Book service and so
  need a Book client the Rust engine does not have yet.

Services whose arithmetic has not been written in Rust. Each one still has a directory, which is
what keeps it in this list: the forty-five that used to sit here without one were services that had
already been deleted, and a to-do list of deleted code is not a plan.

- audit-sampling-service
- biological-assets-service
- disposal-account-service
- distributed-workflow-orchestration-service
- double-entry-principles-service
- encrypted-backup-service
- etl-service
- foreign-exchange-service
- graphql-service
- identity-service
- internal-rate-return-service
- investment-property-service
- net-present-value-service
- payment-gateway-service
- performance-benchmarking-service
- privacy-admin-dashboard-service
- revaluation-model-service
- websocket-service

### Deleted with the maths still unwritten

These were listed as needing their maths ported, and their services were deleted anyway. They are
not one backlog; they are two different situations, and telling them apart changes the work.

`scripts/module_reachability.py` answers which Rust modules any door can reach. A door is the API
server, the native extension, or the bridge they share - nothing else can call into the library. The
script follows imports through crates, dependency renames (`use treasury::{...}` for
`vimbai_service_treasury`) and re-exported functions (`assets::depreciation_batch_from_json`), because
each of those looks unwired to a naive search.

**Answered by maths that is reachable today.** The capability exists, some service calls it, and the
deleted service was one of several front doors to the same arithmetic:

- comparative-financial-statements-service is `ratios_statements::comparative`
- exotic-derivatives-service is `treasury_fx::exotics`
- financial-forecasting-service is `budgeting::forecasting`
- financial-reporting-service and financial-statements-service are `ratios_statements::statements`,
  routed as `financial-statements-prepare` (one body shape, two services that always shared it)
- merger-valuation-service is `capital_valuation::merger`, routed as `merger-valuation-value`
- profit-loss-account-service is `profit_loss` in the budgeting crate, routed as `profit-loss-prepare` and
  `profit-loss-compare`
- scenario-budget-service is `scenario_budget` in the budgeting crate, routed as `scenario-budget-analyze`
- suspense-error-service is `suspense` in the accounting crate, routed as `suspense-error-analyze`
- kpi-tracking-service is `reporting::analysis`, routed as `kpi-tracking-track`
- deal-structuring-service, post-merger-service and synergy-analysis-service share
  `capital_valuation::corporate_events`, routed as `deal-structuring-structure`,
  `post-merger-track` and `synergy-analysis-analyze`
- ifrs-9-financial-instruments-service is `ifrs::instruments`, routed as `ifrs-9-classify`,
  `-sppi-test`, `-business-model`, `-ecl-general`, `-ecl-simplified` and `-hedge-effectiveness`
- ifrs-2-share-based-payment-service is `ifrs::share_based`, routed as `ifrs-2-calculate`,
  `-fair-value-equity-instruments`, `-performance-condition`, `-modification` and
  `-group-transaction`
- ifrs-15-revenue-recognition-service is `ifrs::revenue`, routed as `ifrs-15-recognize`,
  `-variable-consideration`, `-standalone-selling-price`, `-progress-measurement`,
  `-customer-options` and `-modification`
- interim-financial-reporting-service is `ratios_statements::interim_reporting`
- labour-efficiency-variance-service is `cost_accounting::labour_variance`
- liquidity-management-service is `treasury_fx::liquidity`
- management-reporting-service is `reporting::management`
- regulatory-reporting-service is `reporting::regulatory`
- swap-valuation-service is `treasury_fx::derivatives`
- tax-calculation-service is `tax::corporate` behind the `tax-calculate` calculator
- time-value-of-money-service is `capital_valuation::capital_returns`
- treasury-cash-service is `service_treasury::cash_optimization`

**Arithmetic that exists and nothing can reach.** Empty. Every module the reachability script found with
no door has been wired: the last four were business valuation, merger valuation, and the three grouped
into their domain crates. `tests/test_unrouted_maths.py` still fails if a module loses its door while the
tracker says otherwise, and `tests/test_port_tracker.py` fails if a service is claimed wired without being
reachable.

## Retired, and what stands in for them

Samuel asked for the retired Python sources to be deleted, so a service can no longer be its own
evidence. What replaces it is either a recorded request/response capture in
`rust/apps/api-server/tests/parity`, or a calculator registration that answers the same question, or -
for the ones below with neither - a decision recorded here and pinned in `tests/test_port_tracker.py`,
because maths sitting in a crate that nothing can reach is not a port, it is an orphan.

Answered by a capture or a registered calculator:

- accounting-rate-return-service -> capital_valuation::accounting_rate_of_return
- activity-based-costing-service -> calculator
- actual-cost-service -> calculator
- amortization-service -> calculator
- asset-turnover-service -> calculator
- balanced-scorecard-service -> calculator
- budgeted-cost-service -> calculator
- capital-expenditure-budget-service -> calculator
- cash-budget-service -> budgeting::prepare_cash_budget
- consolidation-service -> calculator
- corporate-tax-service -> calculator
- cost-of-capital-service -> capital_valuation::cost_of_capital, capital_valuation::cost_of_capital_percent
- croic-service -> calculator
- cross-currency-swap-service -> treasury_fx::value_cross_currency_swaps
- depreciation-service -> calculator
- discount-factor-service -> capital_valuation::discount_factor, capital_valuation::discount_factor_percent
- divestiture-service -> calculator
- du-pont-service -> calculator
- eps-service -> calculator
- eva-service -> calculator
- financial-distress-prediction-service -> calculator
- fixed-assets-service -> calculator
- flexible-budget-service -> cost_accounting::flexible_budget
- fraud-risk-assessment-service -> calculator
- goodwill-service -> calculator
- government-grants-service -> calculator
- group-tax-service -> calculator
- interest-rate-hedging-service -> advisory::analyze_interest_rate_risk, treasury_fx::analyze_interest_rate_hedge
- internal-control-evaluation-service -> calculator
- inventory-valuation-service -> calculator
- management-accounts-service -> ratios_statements::prepare_management_accounts
- master-budget-service -> budgeting::prepare_master_budget
- multi-currency-service -> treasury_fx::validate_multi_currency_transaction
- net-realizable-value-service -> calculator
- options-pricing-service -> calculator
- partnership-accounting-service -> calculator
- partnership-sale-service -> calculator
- payment-optimization-service -> calculator
- pension-accounting-service -> calculator
- profit-service -> native::profit_json, advisory::analyze_profitability_index
- project-appraisal-service -> capital_valuation::project_appraisal
- regulatory-ratio-service -> calculator
- standard-cost-service -> calculator
- substantive-testing-service -> calculator
- sustainable-growth-service -> capital_valuation::sustainable_growth
- target-costing-service -> calculator
- tax-provision-service -> calculator
- tax-return-preparation-service -> tax::prepare_tax_return
- tax-risk-service -> calculator
- tax-service -> native::group_tax_json, native::tax_provision_json
- throughput-accounting-service -> calculator
- vat-reporting-service -> calculator
- zero-based-budget-service -> budgeting::prepare_zero_based_budget

Maths in a crate with nothing to reach it, kept as a recorded debt:

| service | where the maths is |
| --- | --- |
| annual-report-service | a calculator is not registered |
| bank-fee-analysis-service | ratios_statements::analyze_bank_fees, reporting::analyze_bank_fees |
| capital-budgeting-service | a calculator is not registered |
| cash-flow-forecasting-service | capital_valuation::present_value_of_cash_flows, performance_measures::from_cash_flow |
| credit-derivatives-service | treasury_fx::analyze_credit_derivatives |
| credit-risk-analysis-service | a calculator is not registered |
| discounted-payback-period-service | capital_valuation::discounted_payback |
| equivalent-units-service | cost_accounting::equivalent_units |
| forward-contracts-service | treasury_fx::analyze_forward_contracts |
| ifrs-16-lease-accounting-service | ifrs::calculate_ifrs16 |
| income-statement-service | cost_accounting::marginal_income_statement, ratios_statements::prepare_income_statement |
| internal-controls-testing-service | a calculator is not registered |
| labour-cost-variance-service | cost_accounting::actual_labour_cost, cost_accounting::labour_cost_rate |
| labour-rate-variance-service | cost_accounting::labour_hours_rate, cost_accounting::labour_cost_rate |
| liquidity-forecast-service | a calculator is not registered |
| liquidity-risk-service | a calculator is not registered |
| make-or-buy-decision-service | cost_accounting::analyze_make_or_buy |
| market-risk-service | a calculator is not registered |
| material-cost-variance-service | cost_accounting::actual_material_cost, cost_accounting::material_cost_variance |
| material-price-variance-service | variance::material_price_variance |
| material-usage-variance-service | variance::material_usage_variance |
| payroll-service | equity_payroll::payroll_run, equity_payroll::payroll_taxes |
| r-and-d-tax-service | a calculator is not registered |
| risk-return-analysis-service | capital_valuation::analyze_risk_return |
| sales-price-variance-service | variance::sales_price_volume_analysis |
| sales-volume-variance-service | variance::sales_price_volume_analysis, variance::sales_volume_cm_analysis |
| times-interest-earned-service | ratios_statements::analyze_times_interest_earned |
| treasury-operations-service | treasury_fx::analyze_treasury_operations |
| variance-service | budgeting::analyze_budget_variance, budgeting::detailed_variance_analysis |
| working-capital-finance-service | ratios_statements::analyze_working_capital, ratios_statements::optimize_working_capital |
| working-capital-optimization-service | ratios_statements::analyze_working_capital, ratios_statements::optimize_working_capital |
| working-capital-service | ratios_statements::analyze_working_capital, ratios_statements::optimize_working_capital |
## Divergences worth knowing

`depreciation-service` published a `/calculate` schedule that was synthetic: it was never wired
to the library, and its Python body did not call the seam. The Rust ladder in `vimbai_assets`
is the real implementation, so the figures from that one endpoint were never a like-for-like
reference. The `/method` endpoint was wired and watched by parity tests.

`selling-price-reduction-service` answered `break_even_volume_increase: Infinity` when the new
contribution per unit was not positive. That is not valid JSON, so a strict client saw a parse error
where the service meant "no volume breaks even". The port answers `null` for that case and leaves the
recommendation (`REJECT`/`CONDITIONAL_APPROVAL`) to carry the meaning; every other figure is the same
arithmetic, held in `Money` instead of floats.

`cvp-analysis-service` answered `Infinity` for the figures that do not exist when the contribution
is not positive (break-even volume and revenue, and - in a product mix - break-even and target
revenue). Those are `null` in the port. It also refused a non-positive contribution on
`/target-profit` with 400; the record ports answer 422 for every rejected submission, so that is
what a bad target-profit call gets now.

`overhead-absorption-rate-service` put a `formula` sentence in two of its answers built from Python's
float text ("8000.0 / 1000.0 = 8.0"). Those sentences are gone rather than reworded differently: the
fields beside them carry the same two figures the caller supplied, exactly, and a sentence built from
them would be a third copy to keep in step.

`basis-apportionment-service` multiplied the total by each proportion as a float and summed the
results, so the parts did not always add back to the cost being apportioned. The port splits with
largest-remainder allocation instead: the parts are the total, to six places, and `unapportioned` is
zero whenever there is a basis to spread over. A basis made entirely of zeros still leaves the whole
cost unapportioned, as before.
