"""A contract is only worth having if it is checked against the code it describes.

`contracts/costing.yaml` was written by hand from the handlers. Hand-written files drift, so this
compares it, in both directions, to the Rust that answers the routes:

* every path in the contract is a route the API serves;
* every route of the service the contract covers appears in the contract;
* the money fields are decimal strings, never JSON numbers;
* the keys the contract promises in a response are the keys the handler actually writes.

The last one is the useful one. The first three say the contract is pointed at something real; the
last says the answer it describes is the answer that comes back.
"""

import os
import re

import pytest
import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CONTRACTS = os.path.join(ROOT, "contracts")
API = os.path.join(ROOT, "rust", "apps", "api-server", "src", "records.rs")
HANDLER = os.path.join(ROOT, "rust", "crates", "domains", "costing", "src", "overhead_absorption.rs")

# The prefixes that make up one service's surface. Both spellings are served: the path the service
# publishes, and its mirror under /records. The contract describes the published one.
SERVICE_PREFIX = "/overhead-absorption"


def contract_files():
    return sorted(name for name in os.listdir(CONTRACTS) if name.endswith(".yaml"))


def load(name="costing.yaml"):
    with open(os.path.join(CONTRACTS, name), encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def router_routes():
    """Every path the record router serves, from its own route table."""
    with open(API, encoding="utf-8") as handle:
        text = handle.read()
    return set(re.findall(r'\.route\(\s*"([^"]+)"', text))


def normalise(path):
    """`/records/x/:id` and `/x/:id` are the same route; the contract names it once."""
    return re.sub(r"^/records", "", path)


def resolver(document):
    components = document.get("components", {})

    def resolve(node):
        seen = 0
        while isinstance(node, dict) and "$ref" in node and seen < 20:
            target = node["$ref"]
            assert target.startswith("#/"), f"only local refs are supported, found {target}"
            found = document
            for part in target.lstrip("#/").split("/"):
                found = found[part]
            node = found
            seen += 1
        return node

    return resolve


def properties(document, schema):
    """Every property a response schema can produce, following allOf."""
    resolve = resolver(document)
    schema = resolve(schema)
    found = {}
    for layer in schema.get("allOf", []):
        found.update(properties(document, layer))
    found.update(schema.get("properties", {}))
    return found


def schema_of(document, node):
    return resolver(document)(node)


def test_the_contract_is_openapi_31():
    document = load()
    assert document["openapi"].startswith("3.1"), "the money rules lean on 3.1 nullable types"
    assert contract_files() == ["costing.yaml"], (
        f"a contract was added without a test: {contract_files()}. Either cover it here or say why "
        "it is not checkable against the code."
    )


def test_every_contract_path_is_a_route_the_api_serves():
    document = load()
    served = {normalise(route) for route in router_routes()}
    missing = [path for path in document["paths"] if path not in served]
    assert missing == [], f"the contract describes routes nothing serves: {missing}"


def test_every_route_of_the_covered_service_is_in_the_contract():
    document = load()
    described = set(document["paths"])
    served = {normalise(route) for route in router_routes() if normalise(route).startswith(SERVICE_PREFIX)}
    undescribed = sorted(served - described)
    assert undescribed == [], (
        "the service answers routes the contract does not describe: "
        f"{undescribed}. Adding a route means writing it into the contract."
    )


def test_money_is_always_a_decimal_string():
    """No money field may be a number: a client that parses one loses digits it cannot get back."""
    document = load()
    schemas = document["components"]["schemas"]
    money = schemas["Money"]
    assert money["type"] == "string"
    assert money["pattern"] == "^-?[0-9]+(\\.[0-9]+)?$"

    numeric_names = re.compile(r"(amount|cost|overhead|rate|units|hours|price|value|figure)$")
    offenders = []
    for name, schema in schemas.items():
        if name == "Money":
            continue
        for field, definition in properties(document, schema).items():
            resolved = schema_of(document, definition)
            if resolved.get("$ref", "").endswith("/Money"):
                continue
            if numeric_names.search(field) and resolved.get("type") in ("number", "integer"):
                offenders.append(f"{name}.{field}")
    assert offenders == [], f"these money-like fields are JSON numbers: {offenders}"


def test_every_operation_requires_the_book_scope():
    """Both headers, on every operation.

    A rate that cannot say whose Book it belongs to has no meaning, and the handler refuses one
    without both. OpenAPI cannot express two required api key headers in one scheme, so the caller
    is the document's security scheme and the Book is a parameter - and this asserts the parameter is
    actually there on every operation, which is the half a security scheme cannot carry.
    """
    document = load()
    assert document.get("security"), "no security requirement means no caller is required"
    for path, item in document["paths"].items():
        for method, operation in item.items():
            if method not in ("get", "post", "put", "patch", "delete"):
                continue
            refs = [parameter.get("$ref") for parameter in operation.get("parameters", [])]
            assert "#/components/parameters/BookId" in refs, f"{method.upper()} {path} does not require X-Book-Id"
    book = document["components"]["parameters"]["BookId"]
    assert book["name"] == "X-Book-Id" and book["required"] is True


def test_the_rate_answer_promises_the_keys_the_handler_writes():
    """The contract says what comes back; this reads what the Rust writes."""
    document = load()
    promised = set(properties(document, document["components"]["schemas"]["AbsorptionRate"]))
    with open(HANDLER, encoding="utf-8") as handle:
        text = handle.read()

    body = text[text.index("fn rate_json") : text.index("fn analysis_json")]
    written = set(re.findall(r'"([a-z_]+)":', body))
    assert promised == written, (
        "the contract and the handler disagree about the rate answer. "
        f"only in the contract: {sorted(promised - written)}; "
        f"only in the handler: {sorted(written - promised)}"
    )


def test_the_analysis_answer_promises_the_keys_the_handler_writes():
    document = load()
    promised = set(properties(document, document["components"]["schemas"]["Analysis"]))
    with open(HANDLER, encoding="utf-8") as handle:
        text = handle.read()

    body = text[text.index("fn analysis_json") : text.index("/// The rate itself")]
    written = set(re.findall(r'"([a-z_]+)":', body))
    assert promised == written, (
        f"only in the contract: {sorted(promised - written)}; " f"only in the handler: {sorted(written - promised)}"
    )


@pytest.mark.parametrize(
    "path,schema_name,function",
    [
        ("/overhead-absorption/blanket-rate", "BlanketRateAnswer", "fn blanket_rate"),
        ("/overhead-absorption/absorbed-overhead", "AbsorbedOverheadAnswer", "fn absorbed_overhead"),
    ],
)
def test_each_published_answer_matches_its_written_keys(path, schema_name, function):
    """The arithmetic answers build their JSON inline in the handler, so they are read directly.

    The rate answer and the analysis answer are built by their own functions and are checked above;
    these two never leave the function that computes them.
    """
    document = load()
    promised = set(properties(document, document["components"]["schemas"][schema_name]))
    with open(HANDLER, encoding="utf-8") as handle:
        text = handle.read()

    body = text[text.index(function) :]
    body = body[: body.index("\n}\n")]
    written = set(re.findall(r'"([a-z_]+)":', body))
    if '"analysis_id"' in body:
        written |= {"analysis_id"}
    assert (
        promised <= written
    ), f"{schema_name} promises fields {sorted(promised - written)} that {function} never writes"
