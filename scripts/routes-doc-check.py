#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# `make routes-doc-check`: the web UI's routes against the doc that lists them. Three rules, read as
# text (the daemon's sources, the doc and the page's scripts; nothing runs):
#
# 1. Every `.route("/api/v1/...", get(..) / post(..))` the daemon registers (bins/svolderd/src/web/,
#    tests aside) is a row of the table in docs/usage/web-ui.md ("The routes at a glance") with its
#    method, and every row of the table is a route the daemon serves with that method. A route's
#    segments that vary are written in capitals in the table (`DEVICE`, `N`), `{device}` in the
#    source.
# 2. For the routes in FIELDS (the ones 0.33.0 added or changed: the configured devices, `series`,
#    the device walk, AutoEq and the preset save's `force`) the table's "Takes" cell lists the
#    fields the route reads, and it must be the set of fields of the route's Rust type (an op's
#    variant of the IPC crate, or the handler's own struct; a query's fields are written `?name`)
#    less the keys the path gives. A row of any other route says `not listed` there.
# 3. The page only sends fields a route takes: the keys of the object literals the page's scripts
#    hand each FIELDS route (CORE) are a subset of the route's fields.
#
# 4. Every route of the configured devices, the device walk and AutoEq (the files in GATED) is
#    refused to a reader: its handler calls `writer(` (the gate that answers a guest 401 and a page
#    with writes off 403), directly or through a function of its own file that does.
#
# What it cannot see: a body built apart from the call (`send(path, body)` with `body` made in a
# helper) gives the page's side no keys; a field the Rust type names under `#[serde(rename)]`; a
# route added by a layer other than `.route(` (none today); the meaning of a field, its type, or
# whether the prose around the table says what the table says.
#
# `--self-test` runs the readers over fixed texts and exits 0 when they behave. One line per
# problem; exit 1 when there is any.
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "bins/svolderd/src/web")
DOC = os.path.join(ROOT, "docs/usage/web-ui.md")
WEB = os.path.join(ROOT, "bins/svolderd/web")
IPC = os.path.join(ROOT, "crates/svolder-ipc/src/socket/protocol")

HEADER = ["Method", "Route", "Takes"]

# route (the doc's spelling, no leading slash) -> where its fields are declared:
#   ("enum", file, enum name, variant, keys the path gives[, fields the route reads beyond the
#    variant's: the walk's `run`, which the web route checks and the socket op does not carry])
#   ("struct", file, struct name, keys the path gives)
#   ("none",): the route takes no field
# `?name` is a query field. The fields are the Rust type's, less the path's keys and `op`.
FIELDS = {
    "GET api/v1/devices": ("none",),
    "POST api/v1/devices": ("enum", f"{IPC}/device_config.rs", "DeviceConfigOp", "Add", set()),
    "POST api/v1/devices/DEVICE/edit": ("enum", f"{IPC}/device_config.rs", "DeviceConfigOp", "Update", {"device"}),
    "POST api/v1/devices/DEVICE/remove": ("enum", f"{IPC}/device_config.rs", "DeviceConfigOp", "Remove", {"device"}),
    "POST api/v1/devices/DEVICE/default": ("enum", f"{IPC}/device_config.rs", "DeviceConfigOp", "SetDefault", {"device"}),
    "GET api/v1/devices/DEVICE/series": ("struct", f"{SRC}/api.rs", "SeriesAsked", set(), "?"),
    "POST api/v1/walk": ("none",),
    "POST api/v1/walk/JOB/answer": ("enum", f"{IPC}/walk.rs", "WalkOp", "Answer", {"job"}, {"run"}),
    "POST api/v1/walk/JOB/back": ("enum", f"{IPC}/walk.rs", "WalkOp", "Back", {"job"}, {"run"}),
    "POST api/v1/walk/JOB/cancel": ("enum", f"{IPC}/walk.rs", "WalkOp", "Cancel", {"job"}, {"run"}),
    "POST api/v1/autoeq/search": ("enum", f"{IPC}/autoeq.rs", "AutoEqOp", "Search", set()),
    "POST api/v1/autoeq/preview": ("enum", f"{IPC}/autoeq.rs", "AutoEqOp", "Preview", set()),
    "POST api/v1/autoeq/import": ("enum", f"{IPC}/autoeq.rs", "AutoEqOp", "Import", set()),
    "POST api/v1/autoeq/upload": ("enum", f"{IPC}/autoeq.rs", "AutoEqOp", "Upload", set()),
    "POST api/v1/devices/DEVICE/presets": ("struct", f"{SRC}/writes/presets.rs", "SaveWrite", set(), ""),
}

# route -> [(file under the page's web folder, regex that ends where a call's arguments begin)]:
# the object literals that follow a match are what the page sends the route.
CORE = {
    "api/v1/devices/DEVICE/edit": [("core/device-config-actions.js", r"send\('edit',")],
    "api/v1/devices/DEVICE/remove": [("core/device-config-actions.js", r"send\('remove',")],
    "api/v1/devices/DEVICE/default": [("core/device-config-actions.js", r"send\('default',")],
    "api/v1/walk": [("core/walk-actions.js", r"send\('api/v1/walk',")],
    "api/v1/walk/JOB/answer": [("core/popups/devices/walk-wire.js", r"walk\.answer\(")],
    "api/v1/autoeq/search": [("core/autoeq-actions.js", r"send\('api/v1/autoeq/search',")],
    "api/v1/autoeq/preview": [("core/autoeq-actions.js", r"aboutSide\('api/v1/autoeq/preview',")],
    "api/v1/autoeq/import": [("core/autoeq-actions.js", r"aboutSide\('api/v1/autoeq/import',")],
    "api/v1/autoeq/upload": [("core/autoeq-actions.js", r"send\('api/v1/autoeq/upload',")],
    "api/v1/devices/DEVICE/presets": [
        ("core/autoeq-actions.js", r"devices/\$\{url\}/presets`,"),
        ("core/computer-actions.js", r"devices/\$\{url\}/presets`,"),
    ],
}


def balanced(text, start, open_ch, close_ch):
    """The index just past the bracket that closes the one at `start`, or -1. Strings and
    comments are not read (a bracket in one would fool this; none of the sources has one where it
    matters, which the self-test's fixtures and the real run both show)."""
    depth = 0
    for i in range(start, len(text)):
        if text[i] == open_ch:
            depth += 1
        elif text[i] == close_ch:
            depth -= 1
            if depth == 0:
                return i + 1
    return -1


def normal(path):
    """A route as the doc spells it: no leading slash, a varying segment a capital word."""
    segments = []
    for part in path.strip("/").split("/"):
        if re.fullmatch(r"\{\*?[A-Za-z_]+\}", part):
            part = part.strip("{}*").upper()
        segments.append(part)
    return "/".join(segments)


def served(text, constants=None):
    """`{normal route: {methods}}` of the `.route(` calls in a Rust source, the routes under
    `/api/v1/` only; the part after `#[cfg(test)]` is not read."""
    text = text.split("#[cfg(test)]")[0]
    constants = dict(re.findall(r'const\s+(\w+):\s*&str\s*=\s*"([^"]*)"', text))
    found = {}
    for m in re.finditer(r"\.route\(", text):
        end = balanced(text, m.end() - 1, "(", ")")
        args = text[m.end():end - 1]
        first = re.match(r'\s*(?:"([^"]*)"|(\w+))\s*,', args)
        if not first:
            continue
        path = first.group(1) if first.group(1) is not None else constants.get(first.group(2))
        if not path or not path.startswith("/api/v1/"):
            continue
        rest = args[first.end():]
        methods = {w.upper() for w in re.findall(r"\b(get|post|put|patch|delete|head|options|trace|any)\(", rest)}
        # a route none of the method routers was read for is not compared by method: it is a problem of its own
        methods = methods or {"UNKNOWN"}
        found.setdefault(normal(path), set()).update(methods)
    return found


GATED = ("devices.rs", "walk.rs", "autoeq.rs")


def ungated(text):
    """The handlers of a source whose body does not reach `writer(`: a list of `route (handler)`. A handler
    reaches it when its body holds the call, or calls a function of the file whose body does."""
    text = text.split("#[cfg(test)]")[0]
    constants = dict(re.findall(r'const\s+(\w+):\s*&str\s*=\s*"([^"]*)"', text))

    def body(name):
        m = re.search(rf"\bfn {name}\b[^{{]*\{{", text)
        if not m:
            return ""
        return text[m.end() - 1:balanced(text, m.end() - 1, "{", "}")]

    def gated(name, seen=()):
        b = body(name)
        if "writer(" in b:
            return True
        return any(gated(c, seen + (name,)) for c in set(re.findall(r"\b(\w+)\(", b))
                   if c not in seen and c != name and body(c))
    found = []
    for m in re.finditer(r"\.route\(", text):
        end = balanced(text, m.end() - 1, "(", ")")
        args = text[m.end():end - 1]
        first = re.match(r'\s*(?:"([^"]*)"|(\w+))\s*,', args)
        if not first:
            continue
        path = first.group(1) if first.group(1) is not None else constants.get(first.group(2))
        if not path or not path.startswith("/api/v1/"):
            continue
        for handler in re.findall(r"\b(?:get|post|put|patch|delete)\((\w+)\)", args[first.end():]):
            if not gated(handler):
                found.append(f"{normal(path)} ({handler})")
    return found


def served_in_tree(root):
    found = {}
    for folder, _, files in os.walk(root):
        for name in sorted(files):
            if not name.endswith(".rs") or name in ("tests.rs", "test_support.rs"):
                continue
            with open(os.path.join(folder, name)) as f:
                for route, methods in served(f.read()).items():
                    found.setdefault(route, set()).update(methods)
    return found


def documented(text):
    """`{(method, route): fields}` of the table whose header is Method | Route | Takes; `fields` is
    a set of names, or None for `not listed`. Also the problems found reading it."""
    rows, problems = {}, []
    in_table = False
    for number, line in enumerate(text.splitlines(), 1):
        if not line.startswith("|"):
            in_table = False
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if cells == HEADER:
            in_table = True
            continue
        if not in_table or set("".join(cells)) <= set("-: "):
            continue
        if len(cells) != 3:
            problems.append(f"{DOC}:{number}: a row of the routes' table has {len(cells)} cells, not 3")
            continue
        method, route, takes = cells
        route = route.strip("`")
        if route.startswith("/"):
            route = route[1:]
        key = (method, normal(route))
        if key in rows:
            problems.append(f"{DOC}:{number}: {method} {route} is listed twice")
        if takes == "not listed":
            rows[key] = None
        elif takes.strip("`") == "none":
            rows[key] = set()
        else:
            names = re.findall(r"`([^`]+)`", takes)
            if not names or re.sub(r"`[^`]+`|[, ]", "", takes):
                problems.append(f"{DOC}:{number}: the Takes of {method} {route} is `none`, `not listed` or"
                                " backticked names separated by commas")
                names = []
            rows[key] = set(names)
    return rows, problems


def rust_fields(text, spec):
    """The fields of the Rust type `spec` names, as the table writes them."""
    kind = spec[0]
    if kind == "none":
        return set()
    if kind == "enum":
        _, _, name, variant, path, *beyond = spec
        enum = re.search(rf"pub enum {name}\s*\{{", text)
        if not enum:
            return None
        body = text[enum.end() - 1:balanced(text, enum.end() - 1, "{", "}")]
        var = re.search(rf"^    {variant}\s*(\{{)?", body, re.M)
        if not var or not var.group(1):
            return set()
        block = body[var.end(1) - 1:balanced(body, var.end(1) - 1, "{", "}")]
        fields = set(re.findall(r"^        (?:pub )?(\w+):", block, re.M))
        return (fields - path) | set(beyond[0] if beyond else ())
    _, _, name, path, query = spec
    struct = re.search(rf"struct {name}\s*\{{", text)
    if not struct:
        return None
    block = text[struct.end() - 1:balanced(text, struct.end() - 1, "{", "}")]
    fields = set(re.findall(r"^    (?:pub )?(\w+):", block, re.M))
    return {query + f for f in fields - path}


def literal_keys(text):
    """The keys of every object literal in a piece of JavaScript: `{ a, b: 1, ...c }` gives a and
    b; a spread is not read, a literal inside it is."""
    keys = set()
    for m in re.finditer(r"\{", text):
        end = balanced(text, m.start(), "{", "}")
        if end < 0:
            continue
        inner, depth, entry, entries = text[m.start() + 1:end - 1], 0, "", []
        for ch in inner:
            if ch in "{[(":
                depth += 1
            elif ch in "}])":
                depth -= 1
            if ch == "," and depth == 0:
                entries.append(entry)
                entry = ""
            else:
                entry += ch
        entries.append(entry)
        for e in entries:
            e = e.strip()
            if not e or e.startswith("..."):
                continue
            name = re.match(r"([A-Za-z_]\w*)\s*(?::|$)", e)
            if name:
                keys.add(name.group(1))
    return keys


def sent(text, pattern):
    """The keys of the literals that follow each match of `pattern` in a script, up to the call's
    closing bracket."""
    keys = set()
    for m in re.finditer(pattern, text):
        # the call's own parenthesis opens before the match's end (or at it)
        open_at = text.rfind("(", 0, m.end())
        end = balanced(text, open_at, "(", ")")
        keys |= literal_keys(text[m.end():end - 1 if end > 0 else len(text)])
    return keys


def check(serve, docs, doc_problems, rust, page):
    """The problems. `rust(route)` and `page(route)` give a route's Rust fields and page keys
    (None where there is no source for them)."""
    problems = list(doc_problems)
    for route, methods in sorted(serve.items()):
        for method in sorted(methods):
            if (method, route) not in docs:
                problems.append(f"{DOC}: {method} {route} is served and not in the routes' table")
    for method, route in sorted(docs):
        if method not in serve.get(route, set()):
            problems.append(f"{DOC}: {method} {route} is in the routes' table and not served")
    for key in sorted(FIELDS):
        method, route = key.split(" ", 1)
        if method not in serve.get(route, set()):
            problems.append(f"{route}: FIELDS names {method}, which the daemon does not serve")
            continue
        listed = docs.get((method, route))
        want = rust(key)
        if want is None:
            problems.append(f"{route}: the Rust type {FIELDS[key][2:4]} was not found")
        elif listed is None:
            problems.append(f"{DOC}: {method} {route} must list its fields, not `not listed`")
        elif listed != want:
            problems.append(f"{DOC}: {method} {route} lists {sorted(listed)}, the Rust type takes"
                            f" {sorted(want)}")
    for route in sorted(CORE):
        key = f"POST {route}"
        keys, want = page(route), rust(key)
        if want is None or keys is None:
            continue
        taken = {f.lstrip("?") for f in want} | path_keys(key)
        if not keys <= taken:
            problems.append(f"{route}: the page sends {sorted(keys - taken)}, which the route does not"
                            " take")
    return problems


def path_keys(key):
    """The keys the route's path gives (so a body need not carry them)."""
    spec = FIELDS.get(key)
    if not spec or spec[0] == "none":
        return set()
    return spec[4] if spec[0] == "enum" else spec[3]


def read(path):
    with open(path) as f:
        return f.read()


def run():
    serve = served_in_tree(SRC)
    docs, doc_problems = documented(read(DOC))

    def rust(key):
        spec = FIELDS.get(key)
        if spec is None:
            return None
        return rust_fields(read(spec[1]), spec) if spec[0] != "none" else set()

    def page(route):
        keys = set()
        for rel, pattern in CORE.get(route, []):
            keys |= sent(read(os.path.join(WEB, rel)), pattern)
        return keys

    problems = check(serve, docs, doc_problems, rust, page)
    for folder, _, files in os.walk(os.path.join(SRC, "writes")):
        for name in sorted(files):
            if name in GATED:
                for what in ungated(read(os.path.join(folder, name))):
                    problems.append(f"{os.path.join(folder, name)}: {what} is not behind the writer gate")
    # a pattern that finds nothing in its file would pass every page; it must find its call
    for route, sites in CORE.items():
        for rel, pattern in sites:
            if not re.search(pattern, read(os.path.join(WEB, rel))):
                problems.append(f"{rel}: the call of {route} (/{pattern}/) is not there to read")
    for line in problems:
        print(line)
    if not problems:
        print(f"routes-doc-check: {sum(len(m) for m in serve.values())} routes served and in the table,"
              f" {len(FIELDS)} with their fields checked, {len(CORE)} read on the page's side")
    return 1 if problems else 0


SELF_RUST = '''
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/devices", get(list).post(add))
        .route(
            "/api/v1/devices/{device}/x",
            post(x),
        )
        .route(UPLOAD_PATH, post(up))
        .route("/login", post(login))
        .route("/api/v1/walk/{job}/answer", post(answer))
}
const UPLOAD_PATH: &str = "/api/v1/autoeq/upload";
#[cfg(test)]
mod tests { fn t() { r.route("/api/v1/never", get(x)); } }
'''
SELF_DOC = """
| Method | Route | Takes |
|---|---|---|
| GET | `api/v1/devices` | `none` |
| POST | `api/v1/devices` | `unit`, `name` |
| POST | `api/v1/devices/DEVICE/x` | not listed |
text between
"""
SELF_ENUM = """
pub enum Op {
    List,
    Add {
        /// doc
        unit: String,
        #[serde(skip)]
        name: Option<String>,
    },
    Update {
        device: String,
        name: Option<String>,
    },
}
struct Q {
    ids: Option<String>,
    secs: Option<u64>,
}
"""


def self_test():
    bad = []

    def expect(what, got, want):
        if got != want:
            bad.append(f"{what}: got {got!r}, want {want!r}")

    expect("served", served(SELF_RUST), {
        "api/v1/devices": {"GET", "POST"}, "api/v1/devices/{}/x".replace("{}", "DEVICE"): {"POST"},
        "api/v1/autoeq/upload": {"POST"}, "api/v1/walk/JOB/answer": {"POST"}})
    rows, problems = documented(SELF_DOC)
    expect("documented", rows, {("GET", "api/v1/devices"): set(), ("POST", "api/v1/devices"): {"unit", "name"},
                                ("POST", "api/v1/devices/DEVICE/x"): None})
    expect("documented problems", problems, [])
    expect("an untidy cell is a problem", len(documented(SELF_DOC.replace("`unit`, `name`", "unit and name"))[1]), 1)
    expect("enum add", rust_fields(SELF_ENUM, ("enum", "", "Op", "Add", set())), {"unit", "name"})
    expect("enum update, path key out", rust_fields(SELF_ENUM, ("enum", "", "Op", "Update", {"device"})), {"name"})
    expect("enum unit variant", rust_fields(SELF_ENUM, ("enum", "", "Op", "List", set())), set())
    expect("struct query", rust_fields(SELF_ENUM, ("struct", "", "Q", set(), "?")), {"?ids", "?secs"})
    expect("literal keys", literal_keys("{ query, ...(r ? { refresh: true } : {}), name: 'x' }"),
           {"query", "refresh", "name"})
    expect("sent", sent("send('a', { text, ...(f ? { force: true } : {}) }, sig); send('b', { other })",
                        r"send\('a',"), {"text", "force"})
    # the check itself: a served route that is not listed, a listed one that is not served, a
    # wrong field list and a field the page sends that the route does not take
    serve = {"api/v1/a": {"POST"}}
    gate_src = ('.route("/api/v1/w/a", post(a)).route("/api/v1/w/b", post(b))\n'
                'async fn a() { if let Err(r) = writer(x) {} }\nasync fn b() { run() }\n'
                'async fn run() { other() }\nasync fn c() {}\n')
    expect("a delete route is read", served('.route("/api/v1/x", delete(h)).route("/api/v1/y", get(h).put(h))'),
           {"api/v1/x": {"DELETE"}, "api/v1/y": {"GET", "PUT"}})
    expect("a route with no method is unknown", served('.route("/api/v1/z", handler)'), {"api/v1/z": {"UNKNOWN"}})
    expect("a handler that does not reach the gate", ungated(gate_src), ["api/v1/w/b (b)"])
    saved = dict(FIELDS), dict(CORE)
    try:
        FIELDS.clear()
        CORE.clear()
        got = check(serve, {("POST", "api/v1/b"): None}, [], lambda r: None, lambda r: None)
        expect("missing and extra rows", [p.split(": ", 1)[1] for p in got],
               ["POST api/v1/a is served and not in the routes' table",
                "POST api/v1/b is in the routes' table and not served"])
        FIELDS["POST api/v1/a"] = ("none",)
        CORE["api/v1/a"] = []
        got = check(serve, {("POST", "api/v1/a"): {"x"}}, [], lambda r: {"y"}, lambda r: {"z"})
        expect("fields differ and the page oversteps", len(got), 2)
        got = check(serve, {("POST", "api/v1/a"): {"y"}}, [], lambda r: {"y"}, lambda r: {"y"})
        expect("all agree", got, [])
        got = check(serve, {("POST", "api/v1/a"): None}, [], lambda r: {"y"}, lambda r: set())
        expect("a checked route may not say not listed", len(got), 1)
    finally:
        FIELDS.clear()
        FIELDS.update(saved[0])
        CORE.clear()
        CORE.update(saved[1])
    for line in bad:
        print("self-test:", line)
    print("routes-doc-check self-test:", "FAILED" if bad else "ok")
    return 1 if bad else 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        sys.exit(self_test())
    sys.exit(run())
