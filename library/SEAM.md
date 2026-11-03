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

## Still to do

No service imports the seam yet. Wiring one means the `VIMBAI_CALC_ENGINE`
switch the earlier conversions use: `rust`, `python`, or `auto`, with `/health`
reporting which engine answered.
