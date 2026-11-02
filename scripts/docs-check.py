#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Keep the documentation in a shape a reader can use. `make docs-check` runs this; `--merge`
# (`make docs-check-merge`, run before a merge to main) fails on any docs/ entry that is not a
# shipped doc. One line per problem, `path:line: what`; exit 1 when any is a failure.
#
#   a. every relative Markdown link in README.md, CONTRIBUTING.md and the Markdown under docs/,
#      profiles/ and scripts/ reaches an existing file, and a `#anchor` an existing heading
#      (GitHub's slug rules);
#   b. no process tag (`R12`, `R-pre-3`, the word "ruling", a spec-section pointer such as
#      `§4.13`) in a tracked text file;
#   c. docs/architecture.md stays within 300 lines and each docs/design/*.md within 400;
#   d. with --merge, every entry of docs/ is on the DOCS list;
#   e. every code span in every tracked Markdown file that is shipped, and in the `//` comments of
#      the Rust sources under bins/ and crates/, that looks like a repo path names a tracked file
#      or folder (or the tail of one, at a `/` boundary); PATH_ALLOW lists the spans that are not
#      repo paths, each with its reason.
#
# `--self-test` runs the path matcher of (e) over a fixed list and exits 0 when it behaves.
import glob
import os
import re
import subprocess
import sys
import unicodedata
from urllib.parse import unquote

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Whether a process tag fails the check (it is reported either way).
TAGS_FAIL = True

ARCHITECTURE_MAX = 300
DESIGN_MAX = 400

# The docs a reader gets; anything else under docs/ is a branch's working notes and never reaches
# main.
DOCS = {
    "architecture.md",
    "design",
    "glossary.md",
    "images",
    "known-issues.md",
    "protocol.md",
    "usage",
}

# Files the tag check leaves alone.
TAG_SKIP_FILES = {"Cargo.lock", "scripts/docs-check.py"}
# `path:line` -> why a match there is not a process tag.
TAG_ALLOW = {}

TAG_CASE_RE = re.compile(r"(?<![\w-])(R\d+|R-pre-\w+)(?![\w-])")
RULING_RE = re.compile(r"\brulings?\b", re.IGNORECASE)
# A pointer into a spec section (`§4.13`): the spec is not shipped, so the pointer would dangle.
SECTION_RE = re.compile(r"§\d")


def read(path):
    with open(os.path.join(ROOT, path), encoding="utf-8") as f:
        return f.read()


def markdown_files():
    out = []
    for top in ("README.md", "CONTRIBUTING.md"):
        if os.path.isfile(os.path.join(ROOT, top)):
            out.append(top)
    for d in ("docs", "profiles", "scripts"):
        for p in glob.glob(os.path.join(ROOT, d, "**", "*.md"), recursive=True):
            out.append(os.path.relpath(p, ROOT))
    return sorted(set(out))


def slug(heading):
    h = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", heading)  # a link keeps its text
    h = h.replace("`", "")
    h = re.sub(r"<[^>]+>", "", h)
    h = h.strip().lower()
    out = []
    for ch in h:
        if ch in " ":
            out.append("-")
        elif ch in "-_" or unicodedata.category(ch)[0] in "LN":
            out.append(ch)
    return "".join(out)


def lines_outside_fences(text):
    """Yield (number, line) for each line not inside a fenced code block."""
    fence = None
    for n, line in enumerate(text.split("\n"), 1):
        m = re.match(r"\s*(`{3,}|~{3,})", line)
        if m:
            if fence is None:
                fence = m.group(1)[0] * len(m.group(1))
            elif line.strip().startswith(fence[0] * len(fence)):
                fence = None
            continue
        if fence is None:
            yield n, line


_anchor_cache = {}


def anchors_of(path):
    if path in _anchor_cache:
        return _anchor_cache[path]
    seen = {}
    found = set()
    for _, line in lines_outside_fences(read(path)):
        m = re.match(r" {0,3}#{1,6}\s+(.*?)(?:\s+#+)?\s*$", line)
        if m:
            s = slug(m.group(1))
            k = seen.get(s, 0)
            seen[s] = k + 1
            found.add(s if k == 0 else f"{s}-{k}")
        for a in re.findall(r'<a\s[^>]*?(?:id|name)="([^"]+)"', line):
            found.add(a)
    _anchor_cache[path] = found
    return found


LINK_RE = re.compile(r"(?<!\\)\[(?:[^\[\]]|\[[^\]]*\])*\]\(\s*(<[^>]*>|[^)\s]*)(?:\s+[\"'][^)]*)?\)")


def check_links(problems):
    for path in markdown_files():
        base = os.path.dirname(path)
        for n, line in lines_outside_fences(read(path)):
            line = re.sub(r"`[^`]*`", "", line)  # a link inside a code span is text
            for m in LINK_RE.finditer(line):
                target = m.group(1).strip("<>")
                if not target or re.match(r"[a-z][a-z0-9+.-]*:", target, re.I):
                    continue  # http(s), mailto and any other scheme
                dest, _, frag = target.partition("#")
                dest = unquote(dest)
                if dest:
                    full = os.path.normpath(os.path.join(ROOT, base, dest))
                    rel = os.path.relpath(full, ROOT)
                    if not os.path.exists(full):
                        problems.append((path, n, f"link to {target}: {rel} does not exist"))
                        continue
                else:
                    rel = path
                if frag and rel.endswith(".md") and os.path.isfile(os.path.join(ROOT, rel)):
                    if unquote(frag).lower() not in anchors_of(rel):
                        problems.append((path, n, f"link to {target}: no heading #{frag} in {rel}"))


def not_shipped(path):
    parts = path.split("/")
    return (parts[0] == "docs" and len(parts) > 1 and parts[1] not in DOCS) or parts[0].startswith(".")


def tracked_text_files():
    try:
        out = subprocess.run(
            ["git", "-C", ROOT, "ls-files", "-z"], capture_output=True, check=True
        ).stdout.decode().split("\0")
    except (subprocess.CalledProcessError, OSError):
        sys.exit("docs-check: git ls-files failed; this check needs a git checkout")
    for p in out:
        if not p or p in TAG_SKIP_FILES or not_shipped(p):
            continue
        full = os.path.join(ROOT, p)
        if not os.path.isfile(full) or os.path.islink(full):
            continue
        with open(full, "rb") as f:
            data = f.read()
        if b"\0" in data:
            continue
        try:
            yield p, data.decode("utf-8")
        except UnicodeDecodeError:
            continue


def check_tags(problems):
    for path, text in tracked_text_files():
        for n, line in enumerate(text.split("\n"), 1):
            for m in (
                list(TAG_CASE_RE.finditer(line))
                + list(RULING_RE.finditer(line))
                + list(SECTION_RE.finditer(line))
            ):
                if f"{path}:{n}" in TAG_ALLOW:
                    continue
                problems.append((path, n, f"process tag `{m.group(0)}`"))


def check_sizes(problems):
    limits = [("docs/architecture.md", ARCHITECTURE_MAX)]
    for p in sorted(glob.glob(os.path.join(ROOT, "docs", "design", "*.md"))):
        limits.append((os.path.relpath(p, ROOT), DESIGN_MAX))
    for path, limit in limits:
        if not os.path.isfile(os.path.join(ROOT, path)):
            continue
        n = len(read(path).split("\n")) - 1
        if n > limit:
            problems.append((path, n, f"{n} lines, over the budget of {limit}; split it"))


# Spans that look like a repo path but are not one: `(file, span)` -> why.
PATH_ALLOW = {
    ("docs/architecture.md", "x/tests.rs"): "a placeholder for any module's test file",
    ("CONTRIBUTING.md", "x/tests.rs"): "a placeholder for any module's test file",
    ("CONTRIBUTING.md", "x.rs"): "a placeholder for any module file",
    ("CONTRIBUTING.md", "x/"): "a placeholder for any module folder",
    ("CONTRIBUTING.md", "x/mod.rs"): "a placeholder for the style the repo does not use",
    ("docs/design/code-layout.md", "x/tests.rs"): "a placeholder for any module's test file",
    ("docs/design/code-layout.md", "x.rs"): "a placeholder for any module file",
    ("docs/design/code-layout.md", "x/"): "a placeholder for any module folder",
    ("docs/design/code-layout.md", "x/mod.rs"): "a placeholder for the style the repo does not use",
    ("docs/architecture.md", "x.rs"): "a placeholder for any module file",
    ("docs/architecture.md", "x/"): "a placeholder for any module folder",
    ("docs/architecture.md", "x/mod.rs"): "a placeholder for the style the repo does not use",
    ("docs/architecture.md", "tests/common.rs"): "the file Cargo would build as a test, which the text rules out",
    ("docs/architecture.md", "api/v1/"): "a URL path of the web API",
    ("docs/known-issues.md", "api/v1/"): "a URL path of the web API",
    ("docs/usage/web-ui.md", "api/v1/"): "a URL path of the web API",
    ("docs/usage/web-ui.md", "api/v1/devices/DEVICE/"): "a URL path of the web API",
    ("bins/svolderd/src/web/api.rs", "api/v1/"): "a URL path of the web API",
    ("docs/design/host-audio.md", "svolder/host-eq.toml"): "a file under the user's config folder",
    ("bins/svolder/src/commands/host_eq.rs", "svolder/host-eq.toml"): "a file under the user's config folder",
    ("docs/design/tone-control-and-presets.md", "results/INDEX.md"): "a file of the AutoEq repository, not of this one",
    ("docs/usage/eq.md", "results/INDEX.md"): "a file of the AutoEq repository, not of this one",
    ("crates/svolder-autoeq/src/index.rs", "results/INDEX.md"): "a file of the AutoEq repository, not of this one",
    ("docs/usage/environment.md", "results/"): "a folder of the AutoEq repository, not of this one",
    ("crates/svolder-autoeq/src/client.rs", "results/"): "a folder of the AutoEq repository, not of this one",
    ("crates/svolder-autoeq/src/index.rs", "results/"): "a folder of the AutoEq repository, not of this one",
    ("docs/usage/daemon.md", "./config/svolder/config.toml"): "a path under the user's home, as the text explains",
    ("docs/usage/daemon.md", "config/svolder/config.toml"): "a path under the user's home",
    ("scripts/web-checks/README.md", "target/web-checks/out/"): "a build output folder, not tracked",
    ("bins/svolder/src/env_vars.rs", "target/"): "a build output folder, not tracked",
    ("bins/svolder/src/env_vars.rs", ".git/"): "git's own folder, not tracked",
    ("bins/svolder/src/tui/screenshots.rs", "target/screenshots/"): "a build output folder, not tracked",
    ("bins/svolderd/src/web/icons.rs", "target/web-icons/"): "a build output folder, not tracked",
    ("bins/svolder/src/tui/widgets/rows/tests.rs", "63/"): "a value over a fill, not a path",
    ("crates/svolder-ipc/src/checks/live.rs", "profiles/mine.toml"): "a user's profile file written by the test",
}

PATH_BAD_CHARS = set("*~$<>%{}: …")
PATH_ANY_EXT = (".rs", ".js", ".css", ".py", ".sh", ".html")
PATH_SLASH_EXT = PATH_ANY_EXT + (".toml", ".md", ".json", ".txt", ".svg", ".list", "/")
SPAN_RE = re.compile(r"`([^`\n]+)`")


def path_candidate(span):
    """The repo path a code span stands for, or None when it is not meant as one."""
    if any(c in PATH_BAD_CHARS for c in span) or "://" in span or span[0] in "/-":
        return None
    if not (span.endswith(PATH_ANY_EXT) or ("/" in span and span.endswith(PATH_SLASH_EXT))):
        return None
    if span.startswith("./"):
        span = span[2:]
    return span or None


def path_index(tracked):
    """Every tracked file and folder, with each tail that starts at a `/` boundary."""
    names = set()
    for f in tracked:
        parts = f.split("/")
        for i in range(len(parts)):
            names.add("/".join(parts[i:]))  # the file and its tails
        for j in range(1, len(parts)):
            head = parts[:j]  # the folders that hold it, and their tails
            for i in range(len(head)):
                names.add("/".join(head[i:]))
    names.discard("")
    return names


def path_exists(cand, index):
    return cand.rstrip("/") in index


def span_problems(path, n, line, index, problems):
    for m in SPAN_RE.finditer(line):
        span = m.group(1)
        cand = path_candidate(span)
        if cand is None or (path, span) in PATH_ALLOW or path_exists(cand, index):
            continue
        problems.append((path, n, f"no such path `{span}`"))


def shipped_markdown_files(files):
    return [p for p in files if p.endswith(".md") and not not_shipped(p)]


def tracked_files():
    try:
        out = subprocess.run(
            ["git", "-C", ROOT, "ls-files", "-z"], capture_output=True, check=True
        ).stdout.decode().split("\0")
    except (subprocess.CalledProcessError, OSError):
        sys.exit("docs-check: git ls-files failed; this check needs a git checkout")
    return [p for p in out if p]


def check_paths(problems):
    files = tracked_files()
    index = path_index(files)
    for path in shipped_markdown_files(files):
        for n, line in lines_outside_fences(read(path)):
            span_problems(path, n, line, index, problems)
    for path in files:
        if not path.endswith(".rs") or not path.startswith(("bins/", "crates/")):
            continue
        if not_shipped(path) or not os.path.isfile(os.path.join(ROOT, path)):
            continue
        for n, line in enumerate(read(path).split("\n"), 1):
            if line.lstrip().startswith("//"):
                span_problems(path, n, line, index, problems)


def self_test():
    index = path_index(
        ["bins/svolder/src/main.rs", "bins/svolder/src/tui/keys.rs", "crates/svolder-ble/src/lib.rs"]
    )
    expect = [
        ("bins/svolder/src/main.rs", True, True),
        ("tui/keys.rs", True, True),
        ("keys.rs", True, True),
        ("ys.rs", True, False),
        ("bins/svolder/src/nope.rs", True, False),
        ("crates/", True, True),
        ("~/.config/svolder/config.toml", False, None),
        ("svolder_ipc::config", False, None),
        ("./bins/svolder/src/main.rs", True, True),
        ("./keys.rs", True, True),
        ("https://example.org/x/keys.rs", False, None),
        ("-C/keys.rs", False, None),
        ("--config/keys.rs", False, None),
        ("…/svolder/config.toml", False, None),
        ("config.toml", False, None),
        ("README.md", False, None),
        ("profiles/x.toml", True, False),
        ("docs/x.md", True, False),
        ("nofolder/", True, False),
    ]
    bad = 0
    for span, cand, ok in expect:
        got_cand = path_candidate(span) is not None
        got_ok = got_cand and path_exists(path_candidate(span), index)
        if got_cand != cand or (cand and got_ok != ok):
            print(f"self-test: `{span}`: candidate {got_cand}, passes {got_ok}; "
                  f"expected {cand}, {ok}")
            bad += 1
    return 1 if bad else 0


def main(argv):
    if "--self-test" in argv:
        return self_test()
    merge = "--merge" in argv
    hard, soft = [], []
    check_links(hard)
    check_sizes(hard)
    check_tags(hard if TAGS_FAIL else soft)
    check_paths(hard)
    if merge:
        for entry in sorted(os.listdir(os.path.join(ROOT, "docs"))):
            if entry not in DOCS:
                hard.append((f"docs/{entry}", 1, "not a shipped doc; add it to DOCS in "
                             "scripts/docs-check.py or keep it off main"))
    for path, n, what in hard:
        print(f"{path}:{n}: {what}")
    for path, n, what in soft:
        print(f"{path}:{n}: (warning) {what}")
    return 1 if hard else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
