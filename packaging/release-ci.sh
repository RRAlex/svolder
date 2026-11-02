#!/bin/bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# What the release job does with its draft release, by id (a draft has no tag the API finds, so
# every call names the id). Needs `gh` and GH_TOKEN in the environment, and GITHUB_REPOSITORY.
#   release-ci.sh upload ID FILE...   attach each file to the draft
#   release-ci.sh download ID DIR     fetch every attached file into DIR (to check what GitHub holds)
set -euo pipefail

repo=${GITHUB_REPOSITORY:?}
cmd=${1:?usage: release-ci.sh upload ID FILE... | download ID DIR}
id=${2:?release id}
shift 2

case $cmd in
  upload)
    for f in "$@"; do
      gh api --method POST -H 'Content-Type: application/octet-stream' --input "$f" \
        "https://uploads.github.com/repos/$repo/releases/$id/assets?name=$(basename "$f")" > /dev/null
      echo "uploaded $(basename "$f")"
    done
    ;;
  download)
    dir=${1:?directory}
    mkdir -p "$dir"
    # The listing first, so a failed listing stops the script instead of reading as no assets.
    listing=$(gh api "repos/$repo/releases/$id" --jq '.assets[] | "\(.id) \(.name)"')
    [ -n "$listing" ] || { echo "release-ci: the draft holds no assets" >&2; exit 1; }
    while read -r aid name; do
      gh api -H 'Accept: application/octet-stream' "repos/$repo/releases/assets/$aid" > "$dir/$name"
      echo "downloaded $name"
    done <<< "$listing"
    ;;
  *)
    echo "release-ci: unknown command $cmd" >&2
    exit 1
    ;;
esac
