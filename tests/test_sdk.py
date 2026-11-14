"""The clients are generated from the contracts, so they are checked like generated files.

Two things matter here. A committed client must be exactly what the generator writes, so it cannot be
edited into a second description of the API. And every client must be honest about money: a float
reaching the service would have lost digits before it was sent, so a float is refused.
"""

import ast
import importlib
import importlib.util
import os
import sys

import pytest
import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CONTRACTS = os.path.join(ROOT, "contracts")
sys.path.insert(0, os.path.join(ROOT, "sdk", "python"))


def _generator():
    path = os.path.join(ROOT, "scripts", "generate_sdk.py")
    spec = importlib.util.spec_from_file_location("generate_sdk", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


GENERATOR = _generator()


def contracts():
    return sorted(name for name in os.listdir(CONTRACTS) if name.endswith(".yaml"))


CONTRACT_NAMES = contracts()


def module_for(name):
    return importlib.import_module(GENERATOR.module_name(name)[:-3])


def client_class(name):
    document = yaml.safe_load(open(os.path.join(CONTRACTS, name), encoding="utf-8"))
    return getattr(module_for(name), GENERATOR._class_name(document["info"]["title"]))


class Recorder:
    """Stands in for urlopen and keeps the request it was handed."""

    def __init__(self, body=b"{}"):
        self.requests = []
        self.body = body

    def __call__(self, request, timeout=None):
        self.requests.append(request)
        return self

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def read(self):
        return self.body


def client(name, recorder=None):
    return client_class(name)(
        "https://api.example",
        user_id="user-1",
        book_id="book-1",
        transport=recorder or Recorder(),
    )


def _meaning(source):
    """What a Python file means, with formatting taken out of it.

    This comparison used to be byte for byte, which failed in CI while passing locally: black wrapping
    a line one way on the runner and another way here is not a stale client. Comparing the parsed tree
    keeps every real difference - a method, a docstring, a type check - and drops the one thing that
    was never part of the contract.
    """
    return ast.dump(ast.parse(source))


@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_the_committed_client_matches_the_generator(name):
    path = os.path.join(ROOT, "sdk", "python", GENERATOR.module_name(name))
    assert os.path.isfile(path), f"{path} is missing - run python3 scripts/generate_sdk.py"
    with open(path, encoding="utf-8") as handle:
        committed = handle.read()
    assert _meaning(committed) == _meaning(GENERATOR.render(name)), (
        f"sdk/python/{GENERATOR.module_name(name)} is stale or was edited by hand - "
        "run python3 scripts/generate_sdk.py"
    )


@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_the_client_has_one_method_per_operation(name):
    document = yaml.safe_load(open(os.path.join(CONTRACTS, name), encoding="utf-8"))
    expected = {
        GENERATOR._snake(operation["operationId"])
        for item in document["paths"].values()
        for method, operation in item.items()
        if method in ("get", "post", "put", "patch", "delete")
    }
    klass = client_class(name)
    actual = {
        attribute for attribute in dir(klass) if not attribute.startswith("_") and callable(getattr(klass, attribute))
    }
    assert expected == actual, (
        f"{name}: only in the contract: {sorted(expected - actual)}; "
        f"only in the client: {sorted(actual - expected)}"
    )


@pytest.mark.parametrize("name", CONTRACT_NAMES)
def test_a_client_without_both_headers_is_refused(name):
    for missing in ("user_id", "book_id"):
        kwargs = {"user_id": "user-1", "book_id": "book-1"}
        kwargs[missing] = ""
        with pytest.raises(ValueError):
            client_class(name)("https://api.example", **kwargs)


@pytest.mark.parametrize(
    "name,method_name,arguments",
    [
        (
            "costing.yaml",
            "calculate_absorption_rate",
            {
                "cost_centre_name": "Assembly",
                "cost_centre_id": "cc-1",
                "budgeted_overhead": 125000.5,
                "budgeted_base_units": "4000",
            },
        ),
        (
            "job-costing.yaml",
            "charge_cost",
            {"job_id": "job-1", "amount": 250.75},
        ),
    ],
)
def test_a_float_amount_is_refused_before_it_is_sent(name, method_name, arguments):
    recorder = Recorder()
    with pytest.raises(TypeError) as refusal:
        getattr(client(name, recorder), method_name)(**arguments)
    assert "float" in str(refusal.value)
    assert recorder.requests == [], "a refused amount must never reach the wire"


def test_an_exact_amount_is_sent_as_the_string_it_was_given():
    recorder = Recorder()
    client("costing.yaml", recorder).calculate_absorption_rate(
        cost_centre_name="Assembly",
        cost_centre_id="cc-1",
        budgeted_overhead="125000000000000000000000.25",
        budgeted_base_units="4000",
    )
    request = recorder.requests[0]
    assert request.method == "POST"
    assert (
        "125000000000000000000000.25" in request.data.decode()
    ), "a large amount must travel with every digit it was given"


def test_both_headers_are_sent_and_the_path_is_the_contract_path():
    recorder = Recorder()
    client("costing.yaml", recorder).blanket_rate(
        entity_name="Factory",
        absorption_base="machine_hours",
        total_budgeted_overhead="900000",
        total_base_units="12000",
    )
    request = recorder.requests[0]
    assert request.full_url == "https://api.example/overhead-absorption-rate/blanket-rate"
    headers = {key.lower(): value for key, value in request.header_items()}
    assert headers["x-user-id"] == "user-1"
    assert headers["x-book-id"] == "book-1"


def test_a_query_parameter_is_sent_only_when_given():
    recorder = Recorder()
    client("costing.yaml", recorder).list_absorption_rates()
    assert recorder.requests[0].full_url == "https://api.example/overhead-absorption-rate/rates"

    recorder = Recorder()
    client("costing.yaml", recorder).list_absorption_rates(cost_centre_id="cc-1")
    assert recorder.requests[0].full_url.endswith("/overhead-absorption-rate/rates?cost_centre_id=cc-1")


def test_a_path_parameter_is_substituted_into_the_path():
    """A job's id is part of the route, so it has to arrive in the route and nowhere else."""
    recorder = Recorder()
    client("job-costing.yaml", recorder).charge_cost(job_id="job-1", amount="250.75")
    request = recorder.requests[0]
    assert request.full_url == "https://api.example/job-costing/jobs/job-1/costs"
    assert "job-1" not in request.data.decode(), "the id belongs in the path, not the body"
