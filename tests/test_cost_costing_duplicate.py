"""process-costing-service and product-costing-service are the same service twice.

Both were generated from one template: identical models, identical endpoints, identical
`CostCalculation` label in the graph, and both list by company alone, so each already answered with
the other's records. Rust answers both from one implementation
(`rust/crates/adapters/database/src/cost_calculations.rs`), and this test is what keeps that
justified: if the two Python services ever diverge, the shared port is no longer correct and this
fails rather than quietly serving the wrong shape.

Retiring one of them is a routing decision, not a code one, so nothing here touches services.json.
"""

import pathlib
import re

ROOT = pathlib.Path(__file__).resolve().parents[1]

PAIR = ("process-costing-service", "product-costing-service")

ROUTE = re.compile(r'@app\.(?:get|post|put|delete|patch)\("([^"]+)"')


def source(service, name):
    path = ROOT / service / name
    if not path.exists():
        return None
    return path.read_text()


def routes(service):
    text = source(service, "main.py")
    assert text is not None, f"{service}/main.py is gone; update this test with the retirement"
    return sorted(ROUTE.findall(text))


def strip_bootstrap(text):
    """The import bootstrap at the top of main.py names the service; nothing else should."""
    lines = [
        line
        for line in text.splitlines()
        if "structlog" not in line and "SERVICE_NAME" not in line and "PORT" not in line
    ]
    return "\n".join(lines)


def test_the_two_services_publish_the_same_endpoints():
    first, second = PAIR
    assert routes(first) == routes(second), f"{first} and {second} no longer answer the same paths"


def test_the_two_services_carry_the_same_models():
    """The model files differ only in the sentence naming the service."""
    first, second = PAIR
    shared = [re.sub(r"\b(Process|Product) Costing\b", "Costing", source(service, "models.py")) for service in PAIR]
    assert shared[0] == shared[1], f"{first} and {second} no longer share a model; the shared Rust port needs splitting"


def test_the_two_services_use_the_same_graph_label():
    first, second = PAIR
    label = re.compile(r"CREATE \(x:(\w+)")
    for service in PAIR:
        crud = source(service, "crud.py")
        assert crud is not None, f"{service}/crud.py is gone"
        labels = set(label.findall(crud))
        assert labels == {"CostCalculation"}, f"{service} writes {labels}, not one CostCalculation"


def test_the_two_services_compute_their_totals_the_same_way():
    first, second = PAIR
    for service in PAIR:
        crud = source(service, "crud.py")
        assert (
            "sum(c.amount for c in calc.components)" in crud
        ), f"{service} no longer totals its components by plain addition"
        assert "max(1, calc.quantity)" in crud, f"{service} no longer floors the quantity"
