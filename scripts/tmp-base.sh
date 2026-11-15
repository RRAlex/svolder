# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Sourced by the Makefile recipes and the scripts: every temporary file or directory the repo's
# make targets and scripts make outside the checkout lives under one base directory,
# /tmp/svolder (SVOLDER_TMP_BASE names another, for the self-test).
#
#   svtmp_dir TAG    makes BASE/TAG.XXXXXX, prints its path
#   svtmp_file TAG   makes BASE/TAG.XXXXXX (a file), prints its path
#
# /tmp is shared and sticky, so another local user could pre-create /tmp/svolder or plant a symlink
# there. The base is made 0700 when absent and refused, with a message and a non-zero status,
# unless it is a real directory (not a link) owned by the caller; a mode that is not 0700 on a
# directory the caller owns is set to 0700. When SVOLDER_TMP_LOG names a file, each path made is
# appended to it (make check's manifest, read by `make tmp-leak-check`).

svtmp_base() {
  _svb=${SVOLDER_TMP_BASE:-/tmp/svolder}
  if [ ! -e "$_svb" ] && [ ! -L "$_svb" ]; then
    (umask 077; mkdir -p -- "$_svb") 2>/dev/null || :
  fi
  if [ -L "$_svb" ]; then
    echo "svolder: $_svb is a symbolic link; refusing to use it for temporary files" >&2
    return 1
  fi
  if [ ! -d "$_svb" ]; then
    echo "svolder: $_svb is not a directory; refusing to use it for temporary files" >&2
    return 1
  fi
  if [ "$(stat -c %u -- "$_svb")" != "$(id -u)" ]; then
    echo "svolder: $_svb is not owned by you; refusing to use it for temporary files" >&2
    return 1
  fi
  if [ "$(stat -L -c %a -- "$_svb")" != 700 ]; then
    chmod 700 -- "$_svb" || return 1
    if [ "$(stat -L -c %a -- "$_svb")" != 700 ]; then
      echo "svolder: $_svb is not mode 0700; refusing to use it for temporary files" >&2
      return 1
    fi
  fi
  SVTMP_BASE=$_svb
}

svtmp_log() {
  [ -z "${SVOLDER_TMP_LOG:-}" ] || printf '%s\n' "$1" >>"$SVOLDER_TMP_LOG" 2>/dev/null || :
}

svtmp_dir() {
  svtmp_base || return 1
  _svd=$(mktemp -d "$SVTMP_BASE/$1.XXXXXX") || return 1
  svtmp_log "$_svd"
  printf '%s\n' "$_svd"
}

svtmp_file() {
  svtmp_base || return 1
  _svd=$(mktemp "$SVTMP_BASE/$1.XXXXXX") || return 1
  svtmp_log "$_svd"
  printf '%s\n' "$_svd"
}
