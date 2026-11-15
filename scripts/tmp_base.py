# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
"""The Python side of scripts/tmp-base.sh: the same base (SVOLDER_TMP_BASE, default /tmp/svolder),
made 0700 when absent and refused unless it is a real directory owned by the caller (a wrong mode
on a directory the caller owns is set to 0700); each directory made is appended to SVOLDER_TMP_LOG
when that names a file."""

import os
import stat
import sys
import tempfile


def base():
    path = os.environ.get('SVOLDER_TMP_BASE', '/tmp/svolder')
    if not os.path.lexists(path):
        try:
            os.makedirs(path, mode=0o700, exist_ok=True)
        except OSError:
            pass
    why = None
    try:
        st = os.lstat(path)
    except OSError as e:
        sys.exit('svolder: cannot use %s for temporary files: %s' % (path, e.strerror))
    if stat.S_ISLNK(st.st_mode):
        why = 'is a symbolic link'
    elif not os.path.isdir(path):
        why = 'is not a directory'
    elif st.st_uid != os.getuid():
        why = 'is not owned by you'
    elif stat.S_IMODE(os.stat(path).st_mode) != 0o700:
        os.chmod(path, 0o700)
        if stat.S_IMODE(os.stat(path).st_mode) != 0o700:
            why = 'is not mode 0700'
    if why:
        sys.exit('svolder: %s %s; refusing to use it for temporary files' % (path, why))
    return path


def mkdtemp(tag):
    made = tempfile.mkdtemp(prefix=tag + '.', dir=base())
    log = os.environ.get('SVOLDER_TMP_LOG')
    if log:
        try:
            with open(log, 'a') as out:
                out.write(made + '\n')
        except OSError:
            pass
    return made
