// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// One `/proc` file's bytes out of the entries it holds: NUL after every one, as the
/// kernel writes them.
fn nul_file(entries: &[&str]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in entries {
        bytes.extend_from_slice(entry.as_bytes());
        bytes.push(0);
    }
    bytes
}

#[test]
fn a_nul_separated_file_is_its_entries_without_the_one_its_last_separator_leaves() {
    assert_eq!(
        nul_separated(&nul_file(&["svolderd", "--socket", "/tmp/a.sock"])),
        vec![
            b"svolderd".to_vec(),
            b"--socket".to_vec(),
            b"/tmp/a.sock".to_vec()
        ]
    );
    // An argument that is the empty string is one the program was given, and stays.
    assert_eq!(
        nul_separated(&nul_file(&["svolderd", "--socket", ""])),
        vec![b"svolderd".to_vec(), b"--socket".to_vec(), Vec::new()]
    );
    assert!(nul_separated(&[]).is_empty());
}

#[test]
fn a_process_is_one_of_these_daemons_by_the_name_it_was_started_under() {
    for started in [
        "svolderd",
        "/usr/bin/svolderd",
        "./target/debug/svolderd",
        "/opt/svolder/bin/svolderd",
    ] {
        assert!(
            is_svolderd(&nul_separated(&nul_file(&[started, "-v"]))),
            "{started}"
        );
    }
    for other in ["svolder", "/usr/bin/svolder", "svolderd-sim", "sh"] {
        assert!(
            !is_svolderd(&nul_separated(&nul_file(&[
                other,
                "--socket",
                "/tmp/a.sock"
            ]))),
            "{other}"
        );
    }
    assert!(!is_svolderd(&[]));
}

#[test]
fn the_socket_argument_is_read_in_both_of_its_spellings() {
    let told = |argv: &[&str]| socket_argument(&nul_separated(&nul_file(argv)));
    assert_eq!(
        told(&["svolderd", "--socket", "/tmp/a.sock"]),
        Some(PathBuf::from("/tmp/a.sock"))
    );
    assert_eq!(
        told(&["svolderd", "--socket=/tmp/a.sock", "-v"]),
        Some(PathBuf::from("/tmp/a.sock"))
    );
    // The last one is the one the daemon's own parser read.
    assert_eq!(
        told(&[
            "svolderd",
            "--socket",
            "/tmp/a.sock",
            "--socket=/tmp/b.sock"
        ]),
        Some(PathBuf::from("/tmp/b.sock"))
    );
    // A daemon told nothing, told nothing but a blank, or left mid-argument names no
    // socket of its own.
    assert_eq!(told(&["svolderd", "-v"]), None);
    assert_eq!(told(&["svolderd", "--socket", "", "-v"]), None);
    assert_eq!(told(&["svolderd", "--socket=", "-v"]), None);
    assert_eq!(told(&["svolderd", "--socket"]), None);
    // The program's own path is not an argument, whatever it is called.
    assert_eq!(told(&["--socket=/tmp/a.sock"]), None);
}

#[test]
fn the_socket_variable_is_read_out_of_the_environment_a_daemon_was_started_with() {
    let told = |environ: &[&str]| socket_variable(&nul_separated(&nul_file(environ)));
    assert_eq!(
        told(&["HOME=/home/u", "SVOLDER_SOCKET=/tmp/a.sock", "TERM=xterm"]),
        Some(PathBuf::from("/tmp/a.sock"))
    );
    // The first entry is the one a lookup reads.
    assert_eq!(
        told(&["SVOLDER_SOCKET=/tmp/a.sock", "SVOLDER_SOCKET=/tmp/b.sock"]),
        Some(PathBuf::from("/tmp/a.sock"))
    );
    assert_eq!(told(&["HOME=/home/u"]), None);
    assert_eq!(told(&["SVOLDER_SOCKET="]), None);
    assert_eq!(told(&["SVOLDER_SOCKETS=/tmp/a.sock"]), None);
}

#[test]
fn a_process_is_owned_by_the_first_id_on_its_uid_line() {
    let status = "Name:\tsvolderd\nState:\tS (sleeping)\nUid:\t1000\t1000\t1000\t1000\n\
                      Gid:\t1000\t1000\t1000\t1000\n";
    assert_eq!(owner(status), Some(1000));
    assert_eq!(owner("Name:\tsvolderd\n"), None);
    assert_eq!(owner("Uid:\n"), None);
    assert_eq!(owner("Uid:\tnobody\n"), None);
}

/// Walking the host's own `/proc` answers without panicking and without this process in
/// the answer, whatever is running while the suite does.
#[test]
fn the_running_daemons_of_this_host_are_listed_without_this_process() {
    let me = std::process::id();
    for daemon in running_daemons() {
        assert_ne!(daemon.pid, me);
    }
}

/// The rows of `/proc/net/unix`, as the kernel writes them: a listening socket bound to a
/// path is one of these, and a connected one, an unbound one, one in the abstract
/// namespace and the column heads are not.
#[test]
fn the_listening_sockets_are_read_out_of_the_unix_socket_table() {
    let table = b"Num       RefCount Protocol Flags    Type St Inode Path\n\
0000000000000000: 00000002 00000000 00010000 0001 01 48213 /run/user/1000/svolder.sock\n\
0000000000000000: 00000003 00000000 00000000 0001 03 48307 /run/user/1000/svolder.sock\n\
0000000000000000: 00000002 00000000 00010000 0001 01   917 /tmp/a dir/svolder.sock\n\
0000000000000000: 00000002 00000000 00010000 0001 01 20017 @/tmp/.X11-unix/X0\n\
0000000000000000: 00000002 00000000 00000000 0002 01 20442\n\
0000000000000000: 00000002 00000000 00010000 0001 01 20443\n";
    assert_eq!(
        listening_sockets(table),
        vec![
            (48213, PathBuf::from("/run/user/1000/svolder.sock")),
            (917, PathBuf::from("/tmp/a dir/svolder.sock")),
        ]
    );
    assert!(listening_sockets(b"").is_empty());
    assert!(listening_sockets(b"Num RefCount Protocol Flags Type St Inode Path\n").is_empty());
    // A row cut short names nothing rather than a socket out of the wrong column.
    assert_eq!(
        listening_entry(b"0000000000000000: 00000002 00010000"),
        None
    );
}

/// An `fd` link names a socket by its inode, and anything else it may point at names none.
#[test]
fn a_descriptor_link_names_a_socket_by_its_inode() {
    assert_eq!(socket_inode(b"socket:[48213]"), Some(48213));
    assert_eq!(socket_inode(b"socket:[]"), None);
    assert_eq!(socket_inode(b"socket:[48213"), None);
    assert_eq!(socket_inode(b"pipe:[48213]"), None);
    assert_eq!(socket_inode(b"/dev/null"), None);
    assert_eq!(socket_inode(b"anon_inode:[eventpoll]"), None);
}

/// The socket a process listens on is the one of its sockets the table lists as listening.
#[test]
fn the_socket_a_daemon_listens_on_is_the_listening_one_it_holds() {
    let table = vec![
        (10, PathBuf::from("/run/user/1000/svolder.sock")),
        (20, PathBuf::from("/tmp/other/svolder.sock")),
    ];
    assert_eq!(
        listens_on(&[7, 20, 31], &table),
        Some(PathBuf::from("/tmp/other/svolder.sock"))
    );
    assert_eq!(listens_on(&[7, 31], &table), None);
    assert_eq!(listens_on(&[], &table), None);
}

/// What a daemon was told stands in only where its descriptors or its socket table cannot
/// be read; with both read, the listening socket it holds is the answer, and holding none
/// is no socket, whatever it was told.
#[test]
fn what_a_daemon_was_told_stands_in_only_where_its_sockets_cannot_be_read() {
    let table = vec![(20, PathBuf::from("/run/svolder/svolder.sock"))];
    let told = || Some(PathBuf::from("/tmp/told.sock"));
    assert_eq!(
        listening_path(Some(&[7, 20]), Some(&table), told),
        Some(PathBuf::from("/run/svolder/svolder.sock"))
    );
    assert_eq!(listening_path(Some(&[7]), Some(&table), told), None);
    assert_eq!(
        listening_path(None, Some(&table), told),
        Some(PathBuf::from("/tmp/told.sock")),
        "unreadable descriptors"
    );
    assert_eq!(
        listening_path(Some(&[20]), None, told),
        Some(PathBuf::from("/tmp/told.sock")),
        "an unreadable socket table"
    );
    assert_eq!(listening_path(None, None, || None), None);
}

/// A daemon in a mount namespace of its own is named through its root, where this side
/// reaches that namespace's files; one in this process's own namespace, or whose namespace
/// cannot be read, is named by the path it listens on.
#[test]
fn a_daemon_in_another_mount_namespace_is_named_through_its_root() {
    let ours = Path::new("mnt:[4026531841]");
    let theirs = Path::new("mnt:[4026532690]");
    let listening = || PathBuf::from("/run/svolder/svolder.sock");
    assert_eq!(
        reachable_path(4711, listening(), Some(ours), Some(theirs)),
        PathBuf::from("/proc/4711/root/run/svolder/svolder.sock")
    );
    assert_eq!(
        reachable_path(4711, listening(), Some(ours), Some(ours)),
        listening()
    );
    assert_eq!(
        reachable_path(4711, listening(), Some(ours), None),
        listening()
    );
    assert_eq!(
        reachable_path(4711, listening(), None, Some(theirs)),
        listening()
    );
    // A path that is not absolute says nothing about which root it is under.
    assert_eq!(
        reachable_path(
            4711,
            PathBuf::from("svolder.sock"),
            Some(ours),
            Some(theirs)
        ),
        PathBuf::from("svolder.sock")
    );
}

/// A process of this user's started under the daemon's name and holding a listening socket
/// is found listening on that socket's path — not on the one its environment names, which
/// is only what it was told.
///
/// It is a `sleep` wearing the name, inheriting a listener this test bound: `arg0` is what
/// `/proc` reports and what is read here, and the descriptor is what its `fd` links show,
/// so the walk is driven end to end without a daemon or a radio anywhere near.
#[test]
fn a_process_of_this_users_wearing_the_daemons_name_is_found_on_the_socket_it_holds() {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;

    let dir = crate::test_support::TempPath(
        std::env::temp_dir().join(format!("svolder-processes-{}", std::process::id())),
    );
    std::fs::create_dir_all(&dir).expect("a directory for the socket");
    let listening = dir.join("svolder.sock");
    let _ = std::fs::remove_file(&listening);
    let listener =
        std::os::unix::net::UnixListener::bind(&listening).expect("a socket to listen on");
    let fd = listener.as_raw_fd();
    let mut command = std::process::Command::new("sleep");
    command
        .arg0("svolderd")
        .arg("5")
        .env("SVOLDER_SOCKET", dir.join("told.sock"));
    // The listener is opened close-on-exec. The child clears that flag on its own copy of
    // the descriptor, at whatever number it has, so the socket survives the exec.
    // SAFETY: the closure runs in the child between fork and exec, and calls only fcntl,
    // which is async-signal-safe; the descriptor is open in the parent for as long as the
    // spawn runs, since `listener` outlives it, and so in the child it forked.
    unsafe {
        command.pre_exec(move || match libc::fcntl(fd, libc::F_SETFD, 0) {
            -1 => Err(std::io::Error::last_os_error()),
            _ => Ok(()),
        });
    }
    let mut child = command.spawn().expect("a sleep to wear the daemon's name");

    let found = running_daemons()
        .into_iter()
        .find(|daemon| daemon.pid == child.id());
    let _ = child.kill();
    let _ = child.wait();
    drop(listener);

    assert_eq!(
        found,
        Some(RunningDaemon {
            pid: child.id(),
            socket: Some(listening),
        })
    );
}
