"""The client is generated from the contract, so it is checked like a generated file.

Two things matter here. The committed client must be exactly what the generator writes, so it cannot
be edited into a second description of the API. And it must be honest about money: a float reaching
the service would have lost digits before it was sent, so the client refuses one.
"""

import importlib.util
import os
import sys

import pytest
import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "sdk", "python"))

import costing_client  # noqa: E402


def _generator():
    path = os.path.join(ROOT, "scripts", "generate_sdk.py")
    spec = importlib.util.spec_from_file_location("generate_sdk", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


GENERATOR = _generator()


def contracts():
    directory = os.path.join(ROOT, "contracts")
    return sorted(name for name in os.listdir(directory) if name.endswith(".yaml"))


def client_class():
    return costing_client.VimbaiOverheadAbsorptionClient


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


def client(recorder=None):
    return client_class()(
        "https://api.example",
        user_id="user-1",
        book_id="book-1",
        transport=recorder or Recorder(),
    )


def test_the_committed_clients_match_the_generator():
    for name in contracts():
        path = os.path.join(ROOT, "sdk", "python", GENERATOR.module_name(name))
        assert os.path.isfile(path), f"{path} is missing - run python3 scripts/generate_sdk.py"
        with open(path, encoding="utf-8") as handle:
            committed = handle.read()
        assert committed == GENERATOR.render(name), (
            f"sdk/python/{GENERATOR.module_name(name)} is stale or was edited by hand - "
            "run python3 scripts/generate_sdk.py"
        )


def test_the_client_has_one_method_per_operation():
    with open(os.path.join(ROOT, "contracts", "costing.yaml"), encoding="utf-8") as handle:
        document = yaml.safe_load(handle)

    expected = {
        GENERATOR._snake(operation["operationId"])
        for item in document["paths"].values()
        for method, operation in item.items()
        if method in ("get", "post", "put", "patch", "delete")
    }
    actual = {
        name for name in dir(client_class()) if not name.startswith("_") and callable(getattr(client_class(), name))
    }
    assert expected == actual, (
        f"only in the contract: {sorted(expected - actual)}; " f"only in the client: {sorted(actual - expected)}"
    )


def test_a_client_without_both_headers_is_refused():
    for missing in ({"user_id": "user-1"}, {"book_id": "book-1"}):
        kwargs = {"user_id": "user-1", "book_id": "book-1"}
        kwargs.update({key: "" for key in missing})
        with pytest.raises(ValueError):
            client_class()("https://api.example", **kwargs)


def test_a_float_amount_is_refused_before_it_is_sent():
    recorder = Recorder()
    with pytest.raises(TypeError) as refusal:
        client(recorder).calculate_absorption_rate(
            cost_centre_name="Assembly",
            cost_centre_id="cc-1",
            budgeted_overhead=125000.5,
            budgeted_base_units="4000",
        )
    assert "float" in str(refusal.value)
    assert recorder.requests == [], "a refused amount must never reach the wire"


def test_an_exact_amount_is_sent_as_the_string_it_was_given():
    recorder = Recorder()
    client(recorder).calculate_absorption_rate(
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
    client(recorder).blanket_rate(
        entity_name="Factory",
        absorption_base="machine_hours",
        total_budgeted_overhead="900000",
        total_base_units="12000",
    )
    request = recorder.requests[0]
    assert request.full_url == "https://api.example/overhead-absorption/blanket-rate"
    headers = {key.lower(): value for key, value in request.header_items()}
    assert headers["x-user-id"] == "user-1"
    assert headers["x-book-id"] == "book-1"


def test_a_query_parameter_is_sent_only_when_given():
    recorder = Recorder()
    client(recorder).list_absorption_rates()
    assert recorder.requests[0].full_url == "https://api.example/overhead-absorption/rates"

    recorder = Recorder()
    client(recorder).list_absorption_rates(cost_centre_id="cc-1")
    assert recorder.requests[0].full_url.endswith("/overhead-absorption/rates?cost_centre_id=cc-1")
