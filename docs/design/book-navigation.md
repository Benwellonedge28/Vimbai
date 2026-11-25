# Book navigation
Status: proposed. Existing app-level Book selector remains distinct from the per-Book home. See ../flutter-structure.md and ../domains/organisations.md.

## Hierarchy
App shell → organisations/Books → selected Book home → authorized module → workspace → record/action. Home contains only entry points: bookkeeping, sales, purchases, banking, assets, inventory where supported, budgets, tax, reporting, audit and settings. No balances, charts, transaction lists or financial dashboards on home. Inbox is a dedicated capture workspace, not home.

## Contract
A module descriptor carries stable identifier, parent, label key, route, required permissions, applicable profile and availability. Derive visible entries from effective access and enabled capabilities. Unknown or unavailable modules are not actionable. Deep links recheck authorization; hiding a tile is not security.

## Files
Extend apps/flutter/lib/app service catalogue and shell; retain home_page.dart as Book selection where applicable. Add proposed features/books/presentation/book_home_page.dart and application/navigation resolver only after checking existing equivalents. Feature workspaces own their screens; core/api supplies authenticated Book context. Preserve active Book selection; clear scoped caches on switch.

## Tests
Widget tests for navigation-only layout, profile differences, denied modules, deep links, Book switching, accessibility and offline availability. Server refusal must remain effective if a client bypasses navigation.
