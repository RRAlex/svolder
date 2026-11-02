#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Fail if a dependency's licence is not compatible with distributing svolder under
# GPL-3.0-or-later. `make license-check` runs this; `--self-test` checks the SPDX evaluator
# alone, without cargo.
#
# The check reads `cargo metadata --locked`: every workspace member must be
# GPL-3.0-or-later, every other package must carry an SPDX expression the allow-list
# satisfies. Dev-dependencies are included, a superset of what ships.
import json
import os
import re
import subprocess
import sys
import tomllib

# Each licence here is compatible with distributing svolder under GPL-3.0-or-later.
# packaging/about.toml's `accepted` lists the same set; --self-test fails when they differ.
ALLOWED = {
    "0BSD", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "BSL-1.0", "CC0-1.0",
    "GPL-3.0-or-later", "ISC", "LGPL-2.1-or-later", "LGPL-3.0-or-later", "MIT", "MIT-0",
    "MPL-2.0", "Unicode-3.0", "Unicode-DFS-2016", "Unlicense", "WTFPL", "Zlib",
}
ALLOWED_EXCEPTIONS = {"LLVM-exception"}

# Packages allowed to have no `license` field: {crate name: reason}.
EXCEPTIONS = {}

WORKSPACE_LICENSE = "GPL-3.0-or-later"

# (expression, allowed) pairs run by --self-test.
SELF_TEST = [
    ("MIT OR Apache-2.0", True),
    ("MIT/Apache-2.0", True),
    ("Apache-2.0 / MIT", True),
    ("(MIT OR Apache-2.0) AND Unicode-3.0", True),
    ("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT", True),
    ("MIT AND BSD-3-Clause", True),
    ("GPL-2.0-only", False),
    ("MIT AND GPL-2.0-only", False),
    ("GPL-2.0-only OR MIT", True),
    ("Apache-2.0 WITH Classpath-exception-2.0", False),
    ("", False),
    ("MIT AND (", False),
]

TOKEN = re.compile(r"\s*(?:([A-Za-z0-9.+-]+)|([()/]))")


class ParseError(Exception):
    pass


def tokenize(expr):
    tokens = []
    pos = 0
    expr = expr.rstrip()
    while pos < len(expr):
        m = TOKEN.match(expr, pos)
        if not m:
            raise ParseError(f"unexpected character {expr[pos:].lstrip()[0]!r}")
        pos = m.end()
        word, punct = m.groups()
        if punct == "/":
            tokens.append("OR")  # the old Cargo spelling of OR
        else:
            tokens.append(word or punct)
    return tokens


class Parser:
    """Recursive descent over: or := and (OR and)*, and := with (AND with)*,
    with := (ID [WITH ID] | '(' or ')'). Evaluates while parsing; every branch is
    parsed even when the result is already known, so a malformed tail is still an error."""

    def __init__(self, tokens):
        self.tokens = tokens
        self.pos = 0

    def peek(self):
        return self.tokens[self.pos] if self.pos < len(self.tokens) else None

    def take(self):
        tok = self.peek()
        if tok is None:
            raise ParseError("unexpected end of expression")
        self.pos += 1
        return tok

    def parse_or(self):
        result = self.parse_and()
        while self.peek() == "OR":
            self.take()
            right = self.parse_and()
            result = result or right
        return result

    def parse_and(self):
        result = self.parse_with()
        while self.peek() == "AND":
            self.take()
            right = self.parse_with()
            result = result and right
        return result

    def parse_with(self):
        tok = self.take()
        if tok == "(":
            result = self.parse_or()
            if self.take() != ")":
                raise ParseError("expected ')'")
            return result
        if tok in (")", "AND", "OR", "WITH"):
            raise ParseError(f"unexpected {tok!r}")
        result = tok in ALLOWED
        if self.peek() == "WITH":
            self.take()
            exc = self.take()
            if exc in ("(", ")", "AND", "OR", "WITH"):
                raise ParseError(f"unexpected {exc!r} after WITH")
            result = result and exc in ALLOWED_EXCEPTIONS
        return result


def evaluate(expr):
    """Return (allowed, reason) for an SPDX licence expression."""
    try:
        tokens = tokenize(expr)
        if not tokens:
            return False, "empty licence expression"
        parser = Parser(tokens)
        result = parser.parse_or()
        if parser.peek() is not None:
            raise ParseError(f"unexpected {parser.peek()!r}")
    except ParseError as err:
        return False, f"unparsable licence expression: {err}"
    if result:
        return True, "allowed"
    return False, "no allowed licence satisfies the expression"


def self_test():
    for expr, want in SELF_TEST:
        got, why = evaluate(expr)
        if got != want:
            print(f"self-test failed: {expr!r}: want {want}, got {got} ({why})", file=sys.stderr)
            return 1
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    about = os.path.join(root, "packaging", "about.toml")
    try:
        with open(about, "rb") as f:
            accepted = set(tomllib.load(f).get("accepted", []))
    except (OSError, tomllib.TOMLDecodeError) as err:
        print(f"self-test failed: cannot read {about}: {err}", file=sys.stderr)
        return 1
    if accepted != ALLOWED:
        print(f"self-test failed: packaging/about.toml's accepted differs from ALLOWED:"
              f" only in about.toml {sorted(accepted - ALLOWED)},"
              f" only in ALLOWED {sorted(ALLOWED - accepted)}", file=sys.stderr)
        return 1
    print(f"self-test ok: {len(SELF_TEST)} cases, about.toml in step")
    return 0


def check():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    cargo = os.environ.get("CARGO", "cargo")
    try:
        out = subprocess.run(
            [cargo, "metadata", "--format-version", "1", "--locked"],
            cwd=root, capture_output=True, text=True,
        )
    except OSError as err:
        print(f"check-licenses: cannot run {cargo}: {err}", file=sys.stderr)
        return 1
    if out.returncode != 0:
        print(out.stderr, file=sys.stderr, end="")
        return 1
    try:
        meta = json.loads(out.stdout)
        members = set(meta["workspace_members"])
        packages = meta["packages"]
    except (json.JSONDecodeError, KeyError) as err:
        print(f"check-licenses: unexpected cargo metadata output: {err!r}", file=sys.stderr)
        return 1
    problems = []
    deps = 0
    for pkg in packages:
        lic = pkg.get("license")
        if pkg["id"] in members:
            if lic != WORKSPACE_LICENSE:
                problems.append(
                    f"{pkg['name']} {pkg['version']}: {lic or 'no licence field'}"
                    f" — workspace members must be {WORKSPACE_LICENSE}")
            continue
        deps += 1
        if not lic:
            if pkg["name"] not in EXCEPTIONS:
                problems.append(f"{pkg['name']} {pkg['version']}: no licence field"
                                " — add a license or list the crate in EXCEPTIONS")
            continue
        ok, why = evaluate(lic)
        if not ok:
            problems.append(f"{pkg['name']} {pkg['version']}: {lic} — {why}")
    if problems:
        print("\n".join(problems), file=sys.stderr)
        return 1
    print(f"licences ok: {deps} dependencies, {len(members)} workspace members")
    return 0


if __name__ == "__main__":
    args = sys.argv[1:]
    if args == ["--self-test"]:
        sys.exit(self_test())
    if args:
        print("usage: check-licenses.py [--self-test]", file=sys.stderr)
        sys.exit(2)
    sys.exit(check())
