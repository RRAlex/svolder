#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Check the GitHub Actions workflows in .github/workflows (or the files named on the command
# line) without running them. `make workflow-check` runs this; it fails when:
#   - a file is not valid YAML or has no jobs;
#   - its trigger is anything but `workflow_dispatch` (the release workflow is dormant: it must
#     not start by accident);
#   - an action (`uses:`) is not `owner/repo@<40 hex commit SHA>` followed by a `# vX` comment
#     naming its version;
#   - there is no top-level `permissions: {}`, or a job holds a write scope outside its allowance
#     (`release`: contents; `image` and `manifest`: packages; any other job: none), including
#     `write-all`;
#   - a step that runs `make` has `github.token`, `secrets.` or the bracket forms `github['token']`
#     and `secrets['X']` in its `env`, its script, its job's `env` or the top-level `env` (a build
#     runs with no token);
#   - a `run:` step interpolates `${{ inputs.* }}`, `${{ github.event.* }}` or
#     `${{ github.head_ref }}` (script injection: pass them through `env:`);
#   - a `git ... clone` carries `http.extraheader` after the subcommand (the token would be
#     persisted in .git/config; `git -c ... clone` is transient);
#   - a `make` target a step runs, or a packaging/ or scripts/ file it names, does not exist.
import glob
import os
import re
import sys

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
USES = re.compile(r"^\s*(?:-\s+)?uses:\s*(\S+)\s*(#\s*v\S+.*)?$")
PINNED = re.compile(r"^[\w.-]+/[\w./-]+@[0-9a-f]{40}$")
# The write scopes each job may hold; a job not named holds none.
WRITE_ALLOWED = {"release": {"contents"}, "image": {"packages"}, "manifest": {"packages"}}
TOKEN = re.compile(r"github\.token|secrets\.|github\s*\[\s*['\"]token['\"]\s*\]|secrets\s*\[")


def triggers(doc):
    # YAML 1.1 reads the bare key `on` as the boolean True.
    on = doc.get(True, doc.get("on"))
    if isinstance(on, str):
        return {on}
    if isinstance(on, list):
        return set(on)
    return set(on or {})


def make_targets():
    with open(os.path.join(ROOT, "Makefile"), encoding="utf-8") as f:
        text = f.read()
    return set(re.findall(r"^([a-z0-9-]+):", text, re.M))


def env_text(node):
    return " ".join(str(v) for v in (node.get("env") or {}).values())


def steps(doc):
    for job in (doc.get("jobs") or {}).values():
        for step in job.get("steps") or []:
            yield step


def check(path, targets):
    errors = []
    with open(path, encoding="utf-8") as f:
        text = f.read()
    try:
        doc = yaml.safe_load(text)
    except yaml.YAMLError as e:
        return [f"not valid YAML: {e}"]
    if not isinstance(doc, dict) or not doc.get("jobs"):
        return ["no jobs"]

    found = triggers(doc)
    if found != {"workflow_dispatch"}:
        errors.append(f"triggers are {sorted(found)}; only workflow_dispatch is allowed")

    for n, line in enumerate(text.splitlines(), 1):
        m = USES.match(line)
        if not m:
            continue
        if not PINNED.match(m.group(1)) or not m.group(2):
            errors.append(f"line {n}: `{m.group(1)}` is not owner/repo@<40-hex commit SHA> with a '# vX' comment")

    if "permissions" not in doc or doc["permissions"] != {}:
        errors.append("the top level needs `permissions: {}`")
    for name, job in doc["jobs"].items():
        perms = job.get("permissions")
        if isinstance(perms, str):
            if perms == "write-all":
                errors.append(f"job `{name}` holds write-all")
            continue
        for scope, level in (perms or {}).items():
            if level == "write" and scope not in WRITE_ALLOWED.get(name, set()):
                errors.append(f"job `{name}` holds `{scope}: write`, which its allowance does not include")

    top_env = env_text(doc)
    for job in doc["jobs"].values():
        job_env = env_text(job)
        for step in job.get("steps") or []:
            run = re.sub(r"\$\{\{.*?\}\}", "X", step.get("run", ""))
            run = re.sub(r"^\s*#.*$", "", run, flags=re.M)
            if re.search(r"\bmake\b", run) and (TOKEN.search(top_env) or TOKEN.search(job_env)):
                errors.append("a step that runs make has github.token or secrets.* in its job's or the top-level env")

    for step in steps(doc):
        raw = step.get("run", "")
        if re.search(r"\$\{\{\s*(inputs\.|github\.event\.|github\.head_ref)", raw):
            errors.append("a run step interpolates inputs.*, github.event.* or github.head_ref; pass it through env:")
        if re.search(r"\bclone\b[^\n]*extraheader", raw.replace("\\\n", " ")):
            errors.append("a git clone carries http.extraheader after `clone`; use `git -c ... clone`")
        # Without the ${{ }} expressions and the shell comments.
        run = re.sub(r"\$\{\{.*?\}\}", "X", step.get("run", ""))
        run = re.sub(r"^\s*#.*$", "", run, flags=re.M)
        if re.search(r"\bmake\b", run):
            env = " ".join(str(v) for v in (step.get("env") or {}).values())
            if TOKEN.search(env) or TOKEN.search(step.get("run", "")):
                errors.append("a step that runs make has github.token or secrets.* in its env or script")
        for cmd in re.findall(r"\bmake\b([^\n;&|]*)", run):
            words = [w for w in cmd.split() if not w.startswith("-") and "=" not in w]
            # `make -C DIR target`: the directory is the first word after -C.
            if re.search(r"-C\s+\S+", cmd) and words:
                words = words[1:]
            for w in words:
                if w not in targets:
                    errors.append(f"`make {w}`: no such target in the Makefile")
        for p in re.findall(r"\b(?:packaging|scripts)/[\w./-]+\.(?:sh|py|in)\b", run):
            if not os.path.exists(os.path.join(ROOT, p)):
                errors.append(f"`{p}` does not exist")
    return errors


def main():
    files = sys.argv[1:] or sorted(glob.glob(os.path.join(ROOT, ".github/workflows/*.y*ml")))
    if not files:
        print("workflow-check: no workflow files found")
        return 1
    targets = make_targets()
    failed = False
    for path in files:
        for e in check(path, targets):
            print(f"workflow-check: {os.path.relpath(path)}: {e}")
            failed = True
    if not failed:
        print(f"workflow-check: {len(files)} workflow(s) checked")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
