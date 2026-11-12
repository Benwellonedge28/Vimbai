# The Python seam

`library/native` is the PyO3 extension the services import as `vimbai_native`.
It is the only crate in the project that is not `#![forbid(unsafe_code)]`, because
PyO3's extension machinery needs `unsafe`; no financial logic lives in it.

Every function follows one shape: JSON in, JSON out.

```
payload (dict) -> json.dumps -> native.<name>_json -> json.loads -> response dict
```

The seam turns the payload into the library's request type, calls the library, and
serialises the result. Nothing is recomputed in Python and nothing is recomputed
twice in Rust, so a figure asserted by a Rust test is the figure the service
returns.

## Functions added with the four new crates

| function | library |
| --- | --- |
| `tax_credit_json` | `tax_reporting::credit::calculate` |
| `group_tax_json` | `tax_reporting::group::consolidate` |
| `vat_return_json` | `tax_reporting::vat::prepare` |
| `tax_provision_json` | `tax_reporting::provision::calculate` |
| `tax_risk_json` | `tax_reporting::risk::assess` |
| `grant_json` | `tax_reporting::grants::recognize` |
| `xbrl_json` | `tax_reporting::xbrl::generate` (takes `generated_at` as a second argument) |
| `partnership_allocation_json` | `group_accounting::partnership::allocate` |
| `partner_buyout_json` | `group_accounting::buyout::settle` |
| `partnership_goodwill_json` | `group_accounting::buyout::goodwill` |
| `acquisition_json` | `group_accounting::combination::acquire` |
| `goodwill_impairment_json` | `group_accounting::impairment::test` |
| `pension_json` | `group_accounting::pension::calculate` |
| `consolidation_json` | `group_accounting::consolidation::consolidate` |
| `scorecard_json` | `performance_measures::scorecard::score` |
| `controls_test_json` | `performance_measures::controls::test` |
| `profit_json` | `performance_measures::profit::calculate` |
| `inventory_valuation_json` | `performance_measures::inventory::calculate` |
| `nrv_json` | `performance_measures::realizable::net_realizable_value` |
| `scrap_value_json` | `performance_measures::realizable::scrap_value` |
| `residual_value_json` | `performance_measures::realizable::residual_value` |
| `fx_convert_json` | `performance_measures::fx::convert` (takes `today` as a second argument) |
| `fx_hedge_json` | `performance_measures::fx::hedge` |
| `options_price_json` | `market_risk::options::price` |
| `var_json` | `market_risk::var::calculate` |
| `credit_risk_json` | `market_risk::credit::analyze` |
| `pd_historical_json` | `market_risk::credit::historical_pd` |
| `fraud_risk_json` | `market_risk::fraud::assess` |
| `distress_json` | `market_risk::distress::predict` |
| `regulatory_ratio_json` | `market_risk::regulatory::calculate` |

`consolidation_json` requires the parent's figures, which `consolidation-service`
never had as inputs: the payload carries `parent_revenue`, `parent_profit`,
`parent_assets` and `parent_liabilities`.

## Rebuilding for a local check

```
cd library/native && cargo build
cp target/debug/libvimbai_native.so /tmp/seam/vimbai_native.so
PYTHONPATH=/tmp/seam python3 -c "import vimbai_native"
```

## Verified

Run against the live Python services on canonical payloads, the seam produced the
same figures for the R&D credit, VAT return, partnership allocation, pension
roll-forward and government grant (including the journal narrative strings).

## A bug the port surfaced

`credit-risk-analysis-service` `/analyze` answered 500 to every request. Its
`PDCalculation` model declares `rating_grade` as a string while the endpoint
passed the integer grade, so pydantic raised before the response was built. The
crate renders that field as a string and keeps the top-level grade numeric, so
the seam answers where the service could not. Any client that was retrying
`/analyze` against the Python service has never had a successful response.

## Services crossing the seam

* `r-and-d-tax-service` (`/calculate`), calculator in `calc.py`
* `balanced-scorecard-service` (`/score`), calculator in `calc.py`
* `vat-reporting-service` (`/prepare-return`), calculator in `calc.py`
* `government-grants-service` (`/recognize`), calculator in `calc.py`
* `internal-controls-testing-service` (`/test`), calculator in `calc.py`
* `market-risk-service` (`/var`), calculator in `calc.py`
* `regulatory-ratio-service` (`/calculate`), calculator in `calc.py`
* `financial-distress-prediction-service` (`/predict`), calculator in `calc.py`
* `tax-provision-service` (`/calculate`), calculator in `calc.py`
* `tax-risk-service` (`/assess`), calculator in `calc.py`
* `group-tax-service` (`/consolidate`), calculator in `calc.py`
* `partnership-accounting-service` (`/allocate`), calculator in `calc.py`
* `partnership-sale-service` (`/retirement`, `/goodwill`), calculator in `calc.py`
* `pension-accounting-service` (`/calculate`), calculator in `calc.py`
* `inventory-valuation-service` (`/calculate`), calculator in `calc.py`
* `net-realizable-value-service` (`/calculate-nrv`), calculator in `calc.py`
* `consolidation-service` (`/consolidate`), calculator in `calc.py`
* `options-pricing-service` (`/price`), calculator in `calc.py`
* `fraud-risk-assessment-service` (`/assess`), calculator in `calc.py`
* `du-pont-service` (`/analyze`), `eva-service` (`/calculate`), `croic-service` (`/calculate`),
  `asset-turnover-service` (`/analyze`), each with its own `calc.py`
* `eps-service` (`/calculate`) and `goodwill-service` (`/test`), each with its own `calc.py`
* `corporate-tax-service` (`/calculate`), calculator in `calc.py`
* `depreciation-service` (`/method` only), calculator in `calc.py`

Both read `VIMBAI_CALC_ENGINE`: `auto` (default) uses Rust when the extension is
there, `rust` refuses to start without it, `python` pins the reference. `/health`
reports `calculation_engine`, so production can be checked without inspecting a
response.

Two things the wiring had to get right:

1. The engine probe tests for the *functions* it needs, not the module. An older
   wheel on the image imports cleanly and then fails on the first call, which would
   be a runtime error in a request rather than a clear startup failure.
2. Each service loads its own `calc.py` under a unique module name. A plain
   `import calc` lets whichever service was loaded first own the name, so in a
   process holding two services the second one silently calls the first one's
   calculator. The root suite caught exactly that.

A seam function reports what the library measures, not what a service likes to show.
Where the two differ, the wrapper belongs to the service: consolidation-service adds its
balance sheet split and its fixed cash flow block in Python, around whichever engine
answered, so the library never reports a statement it did not compute.

`tests/test_assurance_seam.py` runs every request through both engines and compares
the figures, including the scorecard's 0-10,000 scale and the control-testing detail
records field by field.

The control-testing detail record is the one place the library had widened a wire
shape: it published `exceptions_found` and `is_effective` where the service had always
published `exceptions` and `effective`. The library now uses the service's names and
keeps the richer diagnostics alongside them, because a client reading `exceptions`
would otherwise have started returning nulls.

## Still to do

Twenty-seven services cross the seam. Several seam functions belong to services that
are already retired, and `depreciation_json` takes a bare asset array plus a method,
so `/calculate` stays unwired on purpose (see lib/SEAM.md notes below). The rest:
the other tax services (provision, risk, group tax), partnership and pension, the
consolidation family, inventory and NRV, FX, and the market and credit risk
services.
