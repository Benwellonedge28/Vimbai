"""
Fake Neo4j driver used by tests: interprets the Cypher patterns used by
crud.py (CREATE with prop wrappers, MATCH with book/user scoping, SET with
parameter/literal assignments, ORDER BY + LIMIT). Shared between the
service test suite and the repo-root integration tests.
"""

import re


class Temporal:
    """Mimics neo4j DateTime/Date temporal values."""

    def __init__(self, iso):
        self._iso = iso

    def iso_format(self):
        return self._iso

    def __str__(self):
        return self._iso


class FakeResult:
    def __init__(self, records=None):
        self._records = records or []

    async def single(self):
        return self._records[0] if self._records else None

    def __aiter__(self):
        self._iter = iter(self._records)
        return self

    async def __anext__(self):
        try:
            return next(self._iter)
        except StopIteration:
            raise StopAsyncIteration


CREATE_NODE_RE = re.compile(r"CREATE \((\w+):(\w+) \{")


class FakeSession:
    def __init__(self):
        self.nodes = []  # dicts: {label, var, props}
        self.edges = []  # (rel_type, user_id, node_ref)

    def _has_edge(self, rel_type, user_id, node):
        return any(e[0] == rel_type and e[1] == user_id and e[2] is node for e in self.edges)

    def _match_rel(self, query):
        m = re.search(r"MATCH \(u:User \{id: \$user_id\}\)-\[:(\w+)\]->", query)
        return m.group(1) if m else None

    def _book_visible(self, node, params):
        return params.get("book_id") is None or node["props"].get("book_id") == params.get("book_id")

    def _extract_props(self, query, params, var, label):
        return self._extract_block_props(query, CREATE_NODE_RE.search(query).start(), params)

    def _extract_block_props(self, query, start, params):
        """Extract props for ONE node-create block (stops at the next CREATE)."""
        tail = query[start + 1 :]
        nxt = tail.find("CREATE ")
        block = query[start : start + 1 + nxt] if nxt != -1 else query[start:]
        props = {}
        for pm in re.finditer(r"(\w+): (?:(toFloat|toInteger|date|datetime)\(\$?(\w+)[^)]*\)?|\$(\w+))", block):
            name, wrapper, pvar, dvar = pm.group(1), pm.group(2), pm.group(3), pm.group(4)
            key = pvar or dvar
            if key not in params:
                continue
            val = params[key]
            if wrapper == "toFloat":
                props[name] = float(val)
            elif wrapper == "toInteger":
                props[name] = int(val)
            elif wrapper in ("date", "datetime"):
                props[name] = Temporal(val)
            else:
                props[name] = val
        return props

    def _resolve_var(self, var, query, params, created):
        """Resolve a Cypher var to a node: created nodes first, then MATCH refs."""
        if var in created:
            return created[var]
        for mm in re.finditer(r"\((\w+):(\w+) \{([^}]*)\}\)", query):
            if mm.group(1) != var:
                continue
            label = mm.group(2)
            eq = {pm.group(1): params.get(pm.group(2)) for pm in re.finditer(r"(\w+): \$(\w+)", mm.group(3))}
            for node in self.nodes:
                if node["label"] == label and all(node["props"].get(k) == v for k, v in eq.items()):
                    return node
        return None

    def _match_nodes(self, query, params):
        m = re.search(
            r"MATCH \(u:User \{\{?id: \$user_id\}\}?\)(?:, |)-\[:\w+\]->\((\w+):(\w+)( \{\{?([^}]*)\}\}?)?", query
        )
        if not m:
            m = re.search(r"MATCH \((\w+):(\w+)(?: \{([^}]*)\})?\)", query)
        if not m:
            raise AssertionError(f"FakeSession: no MATCH pattern in: {query[:100]!r}")
        var, label, match_props = m.group(1), m.group(2), m.group(3) or ""

        eq = {}
        for pm in re.finditer(r"(\w+): \$(\w+)", match_props):
            eq[pm.group(1)] = params.get(pm.group(2))

        found = []
        for node in self.nodes:
            if node["label"] != label:
                continue
            if "user_id" in params:
                # Ownership is either the stamped user_id prop (npo/expense style)
                # or a typed relationship from the user (accounting style).
                if node["props"].get("user_id") is not None:
                    if node["props"]["user_id"] != params["user_id"]:
                        continue
                else:
                    rel_type = self._match_rel(query)
                    if rel_type and not self._has_edge(rel_type, params["user_id"], node):
                        continue
            if any(node["props"].get(k) != v for k, v in eq.items()):
                continue
            if not self._book_visible(node, params):
                continue
            if not self._extra_where(query, params, node, var):
                continue
            found.append((var, node))
        return var, label, found

    def _extra_where(self, query, params, node, var):
        m = re.search(r"\bWHERE (.+?)(?:\n\s*(?:RETURN|SET|CREATE)|\"\"\")", query, re.S)
        if not m:
            return True
        cond = m.group(1)
        props = node["props"]
        for part in re.split(r"\bAND\b", cond):
            part = part.strip()
            if not part or "$book_id IS NULL" in part:
                continue
            pm = re.match(rf"{var}\.(\w+) = \$(\w+)", part)
            if pm:
                if props.get(pm.group(1)) != params.get(pm.group(2)):
                    return False
                continue
            pm = re.match(rf"{var}\.(\w+) = '([^']+)'", part)
            if pm:
                if props.get(pm.group(1)) != pm.group(2):
                    return False
                continue
            # unknown condition: skip
        return True

    async def close(self):
        pass

    async def run(self, query, params=None, **kw):
        merged = dict(params or {})
        merged.update(kw)

        # --- SET path (approve / reject) ---
        sm = re.search(r"\bSET (.+?)(?:\n\s*RETURN|$)", query, re.S)
        if sm:
            var, label, found = self._match_nodes(query, merged)
            set_part = sm.group(1)
            for pm in re.finditer(r"(\w+)\.(\w+) = (\$?)(\w+|'[^']*')", set_part):
                key, is_param, raw = pm.group(2), pm.group(3) == "$", pm.group(4)
                for _, node in found:
                    new_val = merged.get(raw) if is_param else raw.strip("'")
                    if isinstance(node["props"].get(key), Temporal) and isinstance(new_val, str):
                        new_val = Temporal(new_val)
                    node["props"][key] = new_val
            return FakeResult([{v: n["props"]} for v, n in found])

        # --- CREATE path (multi-node: main node + children, e.g. JournalEntry + JournalLines) ---
        cm = CREATE_NODE_RE.search(query)
        if cm:
            created = {}
            records = {}
            for nm in re.finditer(r"CREATE \((\w+):(\w+) \{", query):
                var, label = nm.group(1), nm.group(2)
                props = self._extract_block_props(query, nm.start(), merged)
                node_ref = {"label": label, "var": var, "props": props}
                self.nodes.append(node_ref)
                created[var] = node_ref
                records[var] = props
            # Wire every CREATE (src)-[:REL]->(dst). Sources/targets may be
            # created vars, the user var `u`, or MATCH vars (je_match / a_match).
            # MATCH vars are resolved within the statement segment that owns
            # this edge, so repeated line blocks (a_match: {account_number:
            # $line_N_...}) each bind their own account.
            for em in re.finditer(r"CREATE \((\w+)\)-\[:(\w+)\]->\((\w+)", query):
                src, rel, dst = em.group(1), em.group(2), em.group(3)
                seg_start = query.rfind("MATCH", 0, em.start())
                seg_end = query.find("MATCH", em.end())
                seg = query[seg_start : seg_end if seg_end != -1 else len(query)]
                dst_node = self._resolve_var(dst, seg, merged, created)
                if dst_node is None:
                    continue
                if src == "u" and "user_id" in merged:
                    self.edges.append((rel, merged["user_id"], dst_node))
                    continue
                src_node = self._resolve_var(src, seg, merged, created)
                if src_node is not None:
                    self.edges.append((rel, src_node, dst_node))
            return FakeResult([records])

        # --- COLLECT hydration (get_journal_entry: je + lines via HAS_LINE/IMPACTS) ---
        if "COLLECT({line: jl, account: a}) AS lines_data" in query:
            var, label, found = self._match_nodes(query, merged)
            if not found:
                return FakeResult([])
            v, je_node = found[0]
            lines_data = []
            for rel, frm, to in self.edges:
                if rel == "HAS_LINE" and frm is je_node:
                    account = next(
                        (t for r2, f2, t in self.edges if r2 == "IMPACTS" and f2 is to),
                        None,
                    )
                    lines_data.append({"line": to["props"], "account": account["props"] if account else None})
            if not lines_data:
                lines_data = [{"line": None, "account": None}]
            return FakeResult([{v: je_node["props"], "lines_data": lines_data}])

        # --- MATCH / RETURN path (list, with ORDER BY + LIMIT) ---
        var, label, found = self._match_nodes(query, merged)
        om = re.search(r"ORDER BY (\w+)\.(\w+)( DESC| ASC)?", query)
        if om:
            found.sort(
                key=lambda t: str(t[1]["props"].get(om.group(2))), reverse=bool(om.group(3) and "DESC" in om.group(3))
            )
        lm = re.search(r"LIMIT \$(\w+)", query)
        limit = merged.get(lm.group(1)) if lm else len(found)
        records = [{v: n["props"]} for v, n in found[:limit]]
        return FakeResult(records)


class FakeDriver:
    def __init__(self, session):
        self._session = session

    def session(self, **kw):
        return self._session

    async def close(self):
        pass


def _expense_payload(company_id="comp-1", **overrides):
    payload = {
        "company_id": company_id,
        "employee_id": "emp-1",
        "category": "travel",
        "amount": 1500,
        "description": "Client visit",
        "vendor": "Airline",
    }
    payload.update(overrides)
    return payload
