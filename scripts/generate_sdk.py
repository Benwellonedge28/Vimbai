"""Writes the Python client from the contract.

An SDK written by hand is a second description of the same API, and the two drift. This reads
`contracts/*.yaml` and writes one module per contract, so the client cannot promise a method the
contract does not describe, or miss one it does. `tests/test_sdk.py` fails if the committed client
stops matching the generator.

Nothing here is generated from the route table: the contract is the source, and the contract test is
what keeps the contract honest against the Rust.

Money is never a float. The client refuses one and says why, because a float has already lost digits
by the time it arrives.
"""

import os
import re

import yaml

try:  # black is how the generated modules stay lint-clean without being edited by hand
    import black
except ImportError:  # pragma: no cover - the repository already requires black for CI
    black = None

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CONTRACTS = os.path.join(REPO, "contracts")
OUT = os.path.join(REPO, "sdk", "python")

MONEY_NAME = re.compile(r"(amount|cost|overhead|rate|units|hours|price|value|figure)$")
MONEY_REF = "#/components/schemas/Money"

MODULE_HEADER = '''"""Generated from `contracts/{contract}.yaml` by `scripts/generate_sdk.py`. Do not edit.

{summary}

Amounts travel as decimal strings in both directions, as the contract says. The client refuses a
float: a float has already lost digits by the time it is passed, and an amount that quietly changed
is worse than an error.
"""

from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from decimal import Decimal
from typing import Any, Mapping, Optional


class {class_name}Error(Exception):
    """A refusal from the service, carrying the sentence it was refused with."""

    def __init__(self, status: int, message: str) -> None:
        super().__init__("{{}}: {{}}".format(status, message))
        self.status = status
        self.message = message


def _money(name: str, value: Any) -> str:
    """An amount as the exact string the service expects, or a refusal."""
    if isinstance(value, float):
        raise TypeError(
            "{{}} must be a decimal string, an int or a Decimal, not a float: a float has already "
            "lost digits. Pass it as a string.".format(name)
        )
    if isinstance(value, bool) or not isinstance(value, (str, int, Decimal)):
        raise TypeError(
            "{{}} must be a decimal string, an int or a Decimal, not {{}}".format(
                name, type(value).__name__
            )
        )
    return str(value)


class {class_name}:
    """{summary}

    Both headers are required, and there is no default for either: a rate that cannot say whose Book
    it belongs to has no meaning, so the client refuses to be built without them.
    """

    def __init__(
        self,
        base_url: str,
        *,
        user_id: str,
        book_id: str,
        timeout: Optional[float] = 30.0,
        transport: Optional[Any] = None,
    ) -> None:
        if not user_id or not book_id:
            raise ValueError("user_id and book_id are both required")
        self._base_url = base_url.rstrip("/")
        self._headers = {{"X-User-Id": user_id, "X-Book-Id": book_id}}
        self._timeout = timeout
        self._transport = transport or urllib.request.urlopen

    def _call(
        self,
        method: str,
        path: str,
        query: Optional[Mapping[str, Any]] = None,
        body: Optional[Mapping[str, Any]] = None,
    ) -> Any:
        url = self._base_url + path
        if query:
            url = url + "?" + urllib.parse.urlencode(query)
        data = None if body is None else json.dumps(body).encode("utf-8")
        headers = dict(self._headers)
        if data is not None:
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(url, data=data, method=method, headers=headers)
        try:
            with self._transport(request, timeout=self._timeout) as response:
                payload = response.read()
        except urllib.error.HTTPError as error:
            raw = error.read()
            try:
                message = json.loads(raw).get("error", raw.decode("utf-8", "replace"))
            except (ValueError, AttributeError):
                message = raw.decode("utf-8", "replace")
            raise {class_name}Error(error.code, message) from None
        return json.loads(payload) if payload else None
'''

MODULE_FOOTER = """

__all__ = ["{class_name}", "{class_name}Error"]
"""


def _load(name):
    with open(os.path.join(CONTRACTS, name), encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def _resolve(document, node):
    """Follow local refs, and flatten allOf, so a caller sees the whole schema."""
    seen = 0
    while isinstance(node, dict) and "$ref" in node and seen < 20:
        target = node["$ref"]
        if not target.startswith("#/"):
            raise ValueError(f"only local refs are supported, found {target}")
        found = document
        for part in target.lstrip("#/").split("/"):
            found = found[part]
        node = found
        seen += 1
    merged = dict(node or {})
    if "allOf" in node:
        properties = {}
        required = set()
        for layer in node["allOf"]:
            resolved = _resolve(document, layer)
            properties.update(resolved.get("properties", {}))
            required.update(resolved.get("required", []))
        merged["properties"] = properties
        merged["required"] = sorted(required)
    return merged


def _is_money(document, name, schema):
    if MONEY_REF in str(schema.get("$ref", "")):
        return True
    if "$ref" in schema:
        return _is_money(document, name, _resolve(document, schema))
    return bool(MONEY_NAME.search(name))


def _snake(name):
    partial = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", partial).lower()


def _class_name(title):
    words = re.findall(r"[A-Za-z0-9]+", title)
    return "".join(word.capitalize() for word in words) + "Client"


def _parameters(document, operation):
    """Query and path parameters, both by name."""
    path_params, query_params = [], []
    for raw in operation.get("parameters", []):
        parameter = _resolve(document, raw)
        target = path_params if parameter.get("in") == "path" else query_params
        if parameter.get("name") == "X-Book-Id":
            continue
        target.append(parameter)
    return path_params, query_params


def operations(document):
    """Every operation, in the order the contract lists it: path params, query, body, answer."""
    for path, item in document["paths"].items():
        for method, operation in item.items():
            if method not in ("get", "post", "put", "patch", "delete"):
                continue
            body_schema = {}
            body = operation.get("requestBody", {}).get("content", {}).get("application/json")
            if body:
                body_schema = _resolve(document, body["schema"])
            answer = {}
            for response in operation.get("responses", {}).values():
                content = (response.get("content") or {}).get("application/json")
                if content:
                    answer = _resolve(document, content["schema"])
                    break
            yield path, method, operation, body_schema, answer


def _signature(document, path, method, operation, body_schema):
    path_params, query_params = _parameters(document, operation)
    required = list(body_schema.get("required", []))
    properties = body_schema.get("properties", {})

    arguments = []
    calls = []

    for parameter in path_params:
        arguments.append(f"        {parameter['name']}: str,")
        calls.append(parameter["name"])

    for parameter in query_params:
        if parameter.get("required"):
            arguments.append(f"        {parameter['name']}: str,")
        else:
            arguments.append(f"        {parameter['name']}: Optional[str] = None,")
        calls.append(parameter["name"])

    for name in required:
        arguments.append(f"        {name}: Any,")
    for name, schema in properties.items():
        if name in required:
            continue
        arguments.append(f"        {name}: Optional[Any] = None,")

    return arguments, calls, properties


def _render_operation(document, path, method, operation, body_schema, answer):
    name = _snake(operation["operationId"])
    summary = (operation.get("summary") or "").strip()
    arguments, calls, properties = _signature(document, path, method, operation, body_schema)
    path_params, query_params = _parameters(document, operation)

    lines = [f"    def {name}("]
    lines.append("        self,")
    lines.extend(arguments or [])
    if not arguments:
        lines[-1] = "        self,"
    lines.append("    ) -> Any:")
    if summary:
        lines.append(f'        """{summary}"""')

    if body_schema:
        lines.append("        body: dict[str, Any] = {}")
        for field in properties:
            money = _is_money(document, field, properties[field])
            value = f"_money({field!r}, {field})" if money else field
            lines.append(f"        if {field} is not None:")
            lines.append(f"            body[{field!r}] = {value}")

    query_names = [parameter["name"] for parameter in query_params]
    if query_names:
        lines.append("        query = {")
        for parameter in query_params:
            name_ = parameter["name"]
            lines.append(f"            {name_!r}: {name_},")
        lines.append("        }")
        lines.append("        query = {key: value for key, value in query.items() if value is not None}")
    else:
        lines.append("        query = None")

    if path_params:
        formatted = path
        for parameter in path_params:
            formatted = formatted.replace("{" + parameter["name"] + "}", "{" + parameter["name"] + "}")
        lines.append(f'        path = f"{formatted}"')
    else:
        lines.append(f"        path = {path!r}")

    body_argument = "body" if body_schema else "None"
    lines.append(f"        return self._call({method.upper()!r}, path, query=query, body={body_argument})")
    lines.append("")
    return "\n".join(lines)


def _format(source):
    """Generated code still has to pass the repository's lint job, and it may not be edited by hand.

    Formatting here rather than afterwards keeps `scripts/generate_sdk.py` the only writer: the test
    that compares the committed client to the generator's output compares like for like.
    """
    if black is None:
        return source
    try:
        return black.format_file_contents(source, fast=False, mode=_mode())
    except black.NothingChanged:
        return source


def _mode():
    """The repository's own black settings, so the generator and the lint job agree.

    Reading `pyproject.toml` matters: at the default width the generator's output is clean, and at
    this repository's width it is not.
    """
    if black is None:
        return None
    config = black.find_pyproject_toml((os.path.join(REPO, "pyproject.toml"),))
    if not config:
        return black.Mode()
    settings = black.parse_pyproject_toml(config)
    # The parsed config holds keys that are not `Mode` arguments (target_version among them), and
    # passing them straight through fails. Width is the one that decides the shape of this output.
    return black.Mode(line_length=settings.get("line_length", 88))


def render(name, document=None):
    document = _load(name) if document is None else document
    summary = (document["info"].get("summary") or document["info"]["title"]).strip()
    class_name = _class_name(document["info"]["title"])
    parts = [MODULE_HEADER.format(contract=name[:-5], summary=summary, class_name=class_name)]
    for path, method, operation, body_schema, answer in operations(document):
        parts.append(_render_operation(document, path, method, operation, body_schema, answer))
    parts.append(MODULE_FOOTER.format(class_name=class_name))
    return _format("\n".join(parts))


def module_name(name):
    return name[:-5].replace("-", "_") + "_client.py"


def write():
    os.makedirs(OUT, exist_ok=True)
    written = []
    for name in sorted(n for n in os.listdir(CONTRACTS) if n.endswith(".yaml")):
        path = os.path.join(OUT, module_name(name))
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(render(name))
        written.append(os.path.basename(path))
    package = os.path.join(OUT, "__init__.py")
    if not os.path.exists(package):
        with open(package, "w", encoding="utf-8") as handle:
            handle.write('"""Vimbai service clients. Generated from `contracts/` by scripts/generate_sdk.py."""\n')
    return written


if __name__ == "__main__":
    for name in write():
        print(f"wrote sdk/python/{name}")
