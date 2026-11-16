#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Fetch every catalogue set from its real URL (network), check its size and SHA-256 and that the
# converter accepts it. Run before a release; never part of `make check`. The files go to a
# scratch directory under /tmp/svolder that is deleted at the end.
#
# Usage: catalogue-check.sh [ID...]

set -eu
mkdir -p /tmp/svolder
dir=$(mktemp -d /tmp/svolder/speakers-catalogue.XXXXXX)
trap 'rm -rf "$dir"' EXIT
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" cargo run --quiet -p svolder-speakers --features net \
    --example catalogue_check -- "$dir" "$@"
