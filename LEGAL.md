# Legal and interoperability notice

svolder is an independent, open-source program for controlling Schiit Audio devices that support
Forkbeard, Schiit's Bluetooth Low Energy control protocol, from a Linux computer. Schiit Audio
has not made, endorsed or reviewed it.

This document explains how svolder was developed, and why that development and its publication
fall within the user rights set out in Canada's *Copyright Act*.

## 1. Trademarks

Schiit, Schiit Audio, Forkbeard, and the product names svolder's device profiles carry (Bifrost,
Gungnir, Freya, Kara, Lyr, Mimir, Saga, Tyr, Vidar, Wotan, Yggdrasil and the others) are
trademarks of Schiit Audio. svolder uses these names only to say which devices it works with.
That use implies no affiliation with Schiit Audio, and no sponsorship or endorsement by it.

## 2. Statutory framework: the *Copyright Act* (Canada)

svolder relies on the user rights and exceptions in the
[*Copyright Act*, R.S.C., 1985, c. C-42](https://laws-lois.justice.gc.ca/eng/acts/C-42/),
as amended by Bill C-244 (diagnosis, maintenance and repair) and Bill C-294 (interoperability):

- **Interoperability of computer programs (section 30.61).** Reproducing a lawfully obtained
  computer program in order to get the information needed to make another program interoperable
  with it is not an infringement of copyright.
- **Interoperability and technological protection measures (section 41.12).** This section
  permits circumventing a technological protection measure to make a program, a device or a
  component interoperable with another. svolder does not rely on it: svolder circumvents nothing.

## 3. How svolder was developed

- **A lawfully obtained copy.** The research used the free mobile application Schiit Audio
  publishes for controlling its devices, as distributed. No modified or cracked copy was used.
- **Protocol extraction.** The protocol was learned by static analysis of that application's
  published package (the `.apk`). The analysis produced a description of the protocol: the
  Bluetooth LE GATT services and characteristics, and the message formats and values the
  devices exchange.
- **Clean-room implementation.** svolder was written from that description alone. It contains
  no source code, libraries, binaries, images or other assets from Schiit Audio or its
  application. Every line of it is an original, independent implementation.

## 4. Scope, distribution and licence

- **Only what interoperability needs.** The information drawn from the research is used, and
  published in svolder's source code, only as far as interoperability with the owner's own
  hardware requires. Protocols, message formats and data structures are functional
  requirements for compatibility.
- **Licence.** svolder is free software: you can redistribute it and/or modify it under the
  terms of the GNU General Public License as published by the Free Software Foundation,
  either version 3 of the License, or (at your option) any later version (see `LICENSE`).
- **What svolder does not distribute:** circumvention tools, Schiit Audio's binaries, patches to
  its software, or modified versions of its application.
- **No servers of its own.** svolder talks to the devices over Bluetooth LE on the computer it runs
  on, and to that computer's own audio system when host audio is on. It contacts no server of its
  own, and collects no data. The one exception is yours to make: if you configure the optional MQTT
  bridge, the daemon publishes your devices' state to the broker you name, and nowhere else.
  The MCP server and the web UI, when you turn them on, answer on the addresses you name and
  fetch nothing from elsewhere: the page's fonts and scripts are served by svolderd itself.
- **What the web UI carries.** Besides the crates the binaries are built from, `svolderd` serves
  seven sets of third-party files: IBM Plex Sans and IBM Plex Mono, IBM's split WOFF2 files,
  unmodified, under the SIL Open Font License 1.1 (with "Plex" a Reserved Font Name); Geist
  Medium, the Geist Project Authors' font cut into two Latin subsets (WOFF2, no Reserved Font
  Name), under the same licence; Crimson Pro Regular and Italic, The Crimson Pro Project
  Authors' font, and Caveat Medium, The Caveat Project Authors' font, each cut into two Latin
  subsets (WOFF2, no Reserved Font Name), under the SIL Open Font License 1.1; Russo One and
  Mr Dafoe, each its authors' font as its OFL copyright line names them, unmodified (TrueType),
  under the SIL Open Font License 1.1 (with "Russo" and "Mr Dafoe" Reserved Font Names); and
  htmx 2.0.11, unmodified, under the BSD Zero Clause License. Their licence texts are in
  `THIRD-PARTY-LICENSES`, which the release packages carry, as the crates' and the static
  build's are.

## 5. No warranty, and a safety note

svolder is provided "as is", without warranty of any kind; sections 15 and 16 of the GNU General
Public License, version 3, set out the full terms.

svolder changes device settings, the volume among them. A bug, a wrong device profile or a
mistyped value can send a loud signal, which can damage hearing, headphones or speakers. Start
at a low volume, and keep headphones off your ears while trying a new device or a new version of
svolder. You use svolder at your own risk.

---

*This document explains how the project was developed, for transparency. It is not legal
advice.*
