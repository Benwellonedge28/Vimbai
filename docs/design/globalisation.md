# Globalisation and localisation
Status: proposed extension of common, accounting standards, Flutter and document rendering.

## Model
Locale is presentation preference; jurisdiction is legal policy; currency is monetary identity; timezone is temporal context. Never infer one from another. Use stable identifiers, Unicode text, locale-aware display, explicit fiscal dates and effective-dated jurisdiction packs. Persist exact decimal strings and canonical timestamps independent of display format.

## Integration
Reuse common Money, date and FX primitives; accounting/tax/compliance resolve policy packs; Flutter localization resources own translated labels, plural forms, accessibility and right-to-left layouts; document templates own localized presentation. Keep machine-readable exports stable across locales. Country-specific payment availability belongs to commercial provider configuration.

## Workflow
Add a locale through translation review and rendering tests; add a jurisdiction through expert policy review, fixtures, versioned release and authorized activation. Missing translations fall back visibly to supported language; missing mandatory accounting policy refuses affected operations rather than guessing. Support historical currencies and changes in fiscal rules.

## Acceptance
Test decimal separators, non-Latin scripts, bidirectional text, long labels, timezones and daylight changes, fiscal-year boundaries, currency display, document rendering and localized error messages. Verify identical ledger results across locales. Pilot Zimbabwe first without embedding Zimbabwe-specific assumptions in core.
