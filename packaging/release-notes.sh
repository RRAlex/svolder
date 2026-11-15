#!/bin/bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# The release notes' changelog text for one version, on stdout: the version's section of
# CHANGELOG.md, then each older section down to, not including, the first older version that has
# a tag `v<version>` in the repository, each under `### Also in this release: <version>`. A version
# that was never tagged on its own has its notes carried by the next release this way.
# Without git, outside a repository, or with no tag below the version, only the version's section
# is written, with a notice on stderr. A missing or placeholder section is an error.
#   release-notes.sh <version> [CHANGELOG.md]     from the current directory's repository
#   release-notes.sh --self-test
set -eu

# The body of the section headed `## <version>`, without the blank lines around it.
section_of() { # version file
  awk -v h="## $1" '$0 == h { on = 1; next } /^## / { on = 0 } on' "$2" | sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}'
}

notes() { # version file
  local version=$1 file=$2 body v older=0 tagged=0 extra=""
  body=$(section_of "$version" "$file")
  [ -n "$body" ] || { echo "release-notes: $file has no section '## $version'" >&2; return 1; }
  [ "$body" != 'To be written at the merge.' ] || {
    echo "release-notes: $file's '## $version' section is still the placeholder" >&2
    return 1
  }
  if ! command -v git >/dev/null 2>&1 || ! git rev-parse --git-dir >/dev/null 2>&1; then
    echo "release-notes: no git repository here; only the '## $version' section is used" >&2
  else
    for v in $(sed -n 's/^## \([0-9][^ ]*\)$/\1/p' "$file" | awk -v c="$version" '$0 == c { on = 1; next } on'); do
      if [ -n "$(git tag -l "v$v")" ]; then tagged=1; break; fi
      extra="$extra
### Also in this release: $v

$(section_of "$v" "$file")
"
    done
    if [ "$tagged" = 0 ]; then
      echo "release-notes: no older version has a tag; only the '## $version' section is used" >&2
      extra=""
    fi
  fi
  printf '%s\n' "$body"
  [ -z "$extra" ] || printf '%s\n' "$extra"
}

self_test() {
  local out fail=0 self
  self=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
  . "$(dirname "$0")/../scripts/tmp-base.sh"
  tmp=$(svtmp_dir rn) || return 1
  trap 'rm -rf "$tmp"' EXIT
  cat > "$tmp/CHANGELOG.md" <<'EOT'
# Changelog

## 1.0.3

Three.

- c

## 1.0.2

Two.

## 1.0.1

One.

## 1.0.0

Zero.
EOT
  git init -q "$tmp/r"
  cp "$tmp/CHANGELOG.md" "$tmp/r/"
  git -C "$tmp/r" -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
  git -C "$tmp/r" tag v1.0.0
  out=$(cd "$tmp/r" && "$self" 1.0.3) || fail=1
  case $out in *Three.*"### Also in this release: 1.0.2"*Two.*"### Also in this release: 1.0.1"*One.*) ;; *) fail=1 ;; esac
  case $out in *Zero.* | *"release: 1.0.0"*) fail=1 ;; esac
  git -C "$tmp/r" tag v1.0.2
  out=$(cd "$tmp/r" && "$self" 1.0.3) || fail=1
  case $out in *Three.*) ;; *) fail=1 ;; esac
  case $out in *Two.* | *Also*) fail=1 ;; esac
  # Not a repository: the version's section alone, with a notice.
  out=$(cd "$tmp" && "$self" 1.0.3 2>"$tmp/err") || fail=1
  case $out in *Three.*) ;; *) fail=1 ;; esac
  case $out in *Two.*) fail=1 ;; esac
  [ -s "$tmp/err" ] || fail=1
  # A section that is missing fails.
  (cd "$tmp/r" && "$self" 9.9.9 >/dev/null 2>&1) && fail=1
  [ "$fail" = 0 ] || { echo "release-notes: self-test failed" >&2; return 1; }
  echo "release-notes: self-test ok"
}

if [ "${1:-}" = --self-test ]; then
  self_test
else
  notes "$1" "${2:-CHANGELOG.md}"
fi
