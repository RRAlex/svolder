<!--
SPDX-FileCopyrightText: 2026 the svolder authors
SPDX-License-Identifier: GPL-3.0-or-later
-->
# Security policy

## Supported versions

Only the latest release is supported. A report is checked against it; a flaw found only in an older
release is fixed by upgrading.

## Reporting a vulnerability

Report it privately through GitHub's private vulnerability reporting: open the repository's
Security tab and choose "Report a vulnerability", or go straight to
<https://github.com/RRAlex/svolder/security/advisories/new>. Only the maintainer reads the report.

Please do not open a public issue, pull request or discussion for a security problem, and do not
post details elsewhere before a fix is out. No email address is published for reports.

A useful report says which release it concerns, how the problem is reached, and the smallest steps
that show it.

## What is in scope

- the daemon's web UI and its sign-in tokens;
- the MCP server;
- the daemon's local socket;
- the configuration file and the device passphrase svolder stores on disk;
- the release builds and the container image.

## Known limit, not a reportable flaw

The web page, and anything typed in it (a device's passphrase, a sign-in), travels over plain HTTP
on the local network, as the software is today. Anyone who can watch that network can read it. To
protect it, serve the page behind HTTPS yourself, for example through a reverse proxy. Reports
that only say the web UI is not encrypted are therefore closed as known.

## Out of scope

A Bluetooth device's own protocol weaknesses belong to the device's manufacturer, not to svolder:
svolder speaks the protocol the device offers.

## What to expect

- The aim is a first answer within 30 days. There is no promise of a date for a fix.
- Disclosure is coordinated: the details are made public together with the fix, or when the
  reporter and the maintainer agree.
- The reporter is credited in the release notes if they want to be.
- There is no bounty.
