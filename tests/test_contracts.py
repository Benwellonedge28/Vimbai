"""A contract is only worth having if it is checked against the code it describes.

`contracts/*.yaml` is written by hand from the handlers. Hand-written files drift, so this compares
each one, in both directions, to the Rust that answers the routes:

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
COSTING = os.path.join(ROOT, "rust", "crates", "domains", "costing", "src")

# One entry per contract: the prefix that makes up that service's surface, and the file that answers
# it. Both spellings of the path are served - the one the service publishes and its mirror under
# /records - and the contract describes the published one.
SERVICES = {
    "costing.yaml": {
        "prefix": "/overhead-absorption",
        "handler": os.path.join(COSTING, "overhead_absorption.rs"),
    },
    "job-costing.yaml": {
        "prefix": "/job-costing",
        "handler": os.path.join(COSTING, "job_costing.rs"),
    },
}

CONTRACT_NAMES = sorted(SERVICES)

# The money-like field names. `margin` is deliberately not here: it is a ratio for reading, not a
# balance, and the contracts say so where they use it.
MONEY_NAME = re.compile(r"(amount|cost|overhead|rate|units|hours|price|value|figure)$")


def contract_files():
    return sorted(name for name in os.listdir(CONTRACTS) if name.endswith(".yaml"))


def load(name):
    with open(os.path.join(CONTRACTS, name), encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def router_routes():
    """Every path the record router serves, from its own route table."""
    with open(API, encoding="utf-8") as handle:
        text = handle.read()
    return set(re.findall(r'\.route\(\s*"([^"]+)"', text))


def normalise(path):
    """One spelling per route.

    The router writes a path parameter as `:company_id` and OpenAPI writes it as `{company_id}`; and
    both `/records/x` and `/x` are served, the contract naming the second. Comparing the two needs
    both differences gone.
    """
    path = re.sub(r"^/records", "", path)
    return re.sub(r":([a-z_]+)", r"{\1}", path)


def resolver(document):
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


def required(document, schema):
    resolve = resolver(document)
    schema = resolve(schema)
    names = set(schema.get("required", []))
    for layer in schema.get("allOf", []):
        names |= required(document, layer)
    return names


def schema_of(document, node):
    return resolver(document)(node)


def body_keys(handler, function):
    """The JSON keys a Rust function writes with `json!({...})`."""
    with open(handler, encoding="utf-8") as handle:
        text = handle.read()
    start = text.index(function)
    body = text[start:]
    end = body.find("\n}\n")
    body = body if end < 0 else body[:end]
    # A function may write an answer as an object literal (`"total": x`) or as a record field
    # (`record["total"] = x`); both are the function saying this key comes back.
    return set(re.findall(r'"([a-z_]+)"\]?\s*(?::|=)', body))


def test_every_contract_has_a_place_in_these_checks():
    assert contract_files() == CONTRACT_NAMES, (
        f"a contract was added without saying what serves it: {contract_files()}. Either add it to "
        "SERVICES here or say why it cannot be checked against the code."
    )


@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_the_contract_is_openapi_31(name):
    document = load(name)
    assert document["openapi"].startswith("3.1"), "the money rules lean on 3.1 nullable types"


@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_every_contract_path_is_a_route_the_api_serves(name):
    document = load(name)
    served = {normalise(route) for route in router_routes()}
    missing = [path for path in document["paths"] if path not in served]
    assert missing == [], f"{name} describes routes nothing serves: {missing}"


@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_every_route_of_the_covered_service_is_in_the_contract(name):
    document = load(name)
    prefix = SERVICES[name]["prefix"]
    described = set(document["paths"])
    served = {normalise(route) for route in router_routes() if normalise(route).startswith(prefix)}
    undescribed = sorted(served - described)
    assert undescribed == [], (
        f"{name}: the service answers routes the contract does not describe: {undescribed}. "
        "Adding a route means writing it into the contract."
    )


@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_money_is_always_a_decimal_string(name):
    """No money field may be a number: a client that parses one loses digits it cannot get back."""
    document = load(name)
    schemas = document["components"]["schemas"]
    money = schemas["Money"]
    assert money["type"] == "string", f"{name} lets Money be something other than a string"
    assert money["pattern"] == "^-?[0-9]+(\\.[0-9]+)?$", f"{name} changed the money pattern"

    offenders = []
    for schema_name, schema in schemas.items():
        if schema_name == "Money":
            continue
        for field, definition in properties(document, schema).items():
            resolved = schema_of(document, definition)
            if resolved.get("$ref", "").endswith("/Money"):
                continue
            if MONEY_NAME.search(field) and resolved.get("type") in ("number", "integer"):
                offenders.append(f"{schema_name}.{field}")
    assert offenders == [], f"{name}: these money-like fields are JSON numbers: {offenders}"


def test_every_contract_states_the_same_money_rule():
    """Copied schemas drift. Every contract has to describe an amount the same way."""
    declared = {name: load(name)["components"]["schemas"]["Money"] for name in CONTRACT_NAMES}
    first = declared[CONTRACT_NAMES[0]]
    for name, money in declared.items():
        assert money["type"] == first["type"] and money["pattern"] == first["pattern"], (
            f"{name} describes money differently from {CONTRACT_NAMES[0]}, so a client generated "
            "from one would refuse amounts the other accepts"
        )


@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_every_operation_requires_the_book_scope(name):
    """Both headers, on every operation.

    A figure that cannot say whose Book it belongs to has no meaning, and the handlers refuse one
    without both. OpenAPI cannot express two required api key headers in one scheme, so the caller is
    the document's security scheme and the Book is a parameter - and this asserts the parameter is
    actually there on every operation, which is the half a security scheme cannot carry.
    """
    document = load(name)
    assert document.get("security"), f"{name} requires no caller at all"
    for path, item in document["paths"].items():
        for method, operation in item.items():
            if method not in ("get", "post", "put", "patch", "delete"):
                continue
            refs = [parameter.get("$ref") for parameter in operation.get("parameters", [])]
            assert (
                "#/components/parameters/BookId" in refs
            ), f"{name}: {method.upper()} {path} does not require X-Book-Id"
    book = document["components"]["parameters"]["BookId"]
    assert book["name"] == "X-Book-Id" and book["required"] is True


def test_the_rate_answer_promises_the_keys_the_handler_writes():
    """The contract says what comes back; this reads what the Rust writes."""
    document = load("costing.yaml")
    promised = set(properties(document, document["components"]["schemas"]["AbsorptionRate"]))
    written = body_keys(SERVICES["costing.yaml"]["handler"], "fn rate_json")
    assert promised == written, (
        "the contract and the handler disagree about the rate answer. "
        f"only in the contract: {sorted(promised - written)}; "
        f"only in the handler: {sorted(written - promised)}"
    )


def test_the_analysis_answer_promises_the_keys_the_handler_writes():
    document = load("costing.yaml")
    promised = set(properties(document, document["components"]["schemas"]["Analysis"]))
    written = body_keys(SERVICES["costing.yaml"]["handler"], "fn analysis_json")
    assert promised == written, (
        f"only in the contract: {sorted(promised - written)}; " f"only in the handler: {sorted(written - promised)}"
    )


@pytest.mark.parametrize(
    "schema_name,function",
    [
        ("BlanketRateAnswer", "fn blanket_rate"),
        ("AbsorbedOverheadAnswer", "fn absorbed_overhead"),
    ],
)
def test_each_inline_costing_answer_matches_its_written_keys(schema_name, function):
    """Two answers build their JSON inline in the handler, so they are read directly."""
    document = load("costing.yaml")
    promised = set(properties(document, document["components"]["schemas"][schema_name]))
    written = body_keys(SERVICES["costing.yaml"]["handler"], function)
    if "analysis_id" in written:
        written |= {"analysis_id"}
    assert (
        promised <= written
    ), f"{schema_name} promises fields {sorted(promised - written)} that {function} never writes"


@pytest.mark.parametrize(
    "schema_name,function",
    [
        ("JobList", "fn jobs"),
        ("ChargeAnswer", "fn add_cost"),
        ("Profitability", "fn profitability"),
    ],
)
def test_each_job_costing_answer_matches_its_written_keys(schema_name, function):
    """Job costing answers are built inline too, and a job's own fields pass through the store."""
    document = load("job-costing.yaml")
    promised = set(properties(document, document["components"]["schemas"][schema_name]))
    written = body_keys(SERVICES["job-costing.yaml"]["handler"], function)
    assert (
        promised <= written
    ), f"{schema_name} promises fields {sorted(promised - written)} that {function} never writes"


def test_a_job_records_the_figures_the_contract_guarantees():
    """The four figures a job always carries, plus the ones it is opened with."""
    document = load("job-costing.yaml")
    job = document["components"]["schemas"]["Job"]
    promised = set(job["properties"])
    written = body_keys(SERVICES["job-costing.yaml"]["handler"], "fn create_job")
    guaranteed = {"company_id", "status", "contract_value", "total_cost", "gross_profit", "gross_margin"}
    assert guaranteed <= promised, f"the Job schema lost figures: {sorted(guaranteed - promised)}"
    assert guaranteed <= written, f"create_job no longer writes: {sorted(guaranteed - written)}"
    assert required(document, job) == {"company_id", "contract_value", "total_cost", "gross_profit", "gross_margin"}
