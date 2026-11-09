# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# The runtime image: the static binaries `make docker` builds into target/dist, the same bytes
# the .deb, the .rpm and the tarball carry, with no shared library to install beside them.
# The base image is pinned by digest, as the build container's is.
FROM ubuntu:26.04@sha256:f3d28607ddd78734bb7f71f117f3c6706c666b8b76cbff7c9ff6e5718d46ff64
# The user the entrypoint drops to by default (PUID/PGID override it at run time); `make docker`
# passes the host user's uid. The base image already carries a user at 1000, so whoever holds
# the requested uid is removed first.
ARG UID=1000
ARG VERSION=0.0.0
ARG CREATED=1970-01-01T00:00:00Z
LABEL org.opencontainers.image.title="svolder" \
      org.opencontainers.image.description="Control Schiit Forkbeard devices over Bluetooth LE: svolderd, with the svolder CLI beside it" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.created="${CREATED}" \
      org.opencontainers.image.licenses="GPL-3.0-or-later" \
      org.opencontainers.image.source="https://github.com/RRAlex/svolder" \
      org.opencontainers.image.vendor="the svolder authors"
RUN taken=$(getent passwd ${UID} | cut -d: -f1) && { [ -z "$taken" ] || userdel -r "$taken"; } \
 && useradd --uid ${UID} --create-home svolder && mkdir -p /config /run/svolder && chown svolder /config /run/svolder
# Group 0 may write what the user may, so a compose `user: N:0` start (which skips the entrypoint's
# repair and drop) can write the folders too.
RUN chgrp 0 /config /run/svolder && chmod g=u /config /run/svolder
# The PipeWire clients the computer's volume and EQ spawn (pw-cli, pw-dump, pw-record, pipewire,
# wpctl); no PipeWire daemon runs here, the host's runtime directory is mounted in
# (docker-compose.host-audio.yml).
RUN apt-get update && apt-get install -y --no-install-recommends pipewire-bin wireplumber \
 && rm -rf /var/lib/apt/lists/*
# The CLI binary sits out of PATH behind a wrapper of its own name (the entrypoint script, which
# runs it as PUID:PGID), so `docker exec svolder svolder ...` runs as the daemon's user.
COPY target/dist/svolderd /usr/local/bin/
COPY target/dist/svolder /usr/local/libexec/svolder
COPY packaging/docker-entrypoint.sh /usr/local/bin/docker-entrypoint
RUN ln -s docker-entrypoint /usr/local/bin/svolder
COPY LICENSE target/dist/THIRD-PARTY-LICENSES /usr/share/licenses/svolder/
# The image starts as root so the entrypoint can repair ./config and ./run when Docker made them
# root-owned, then runs svolderd as PUID:PGID (default 1000:1000). The web page and the MCP server
# listen on every interface by default (under the compose file's host networking, the host's: the
# page is reached on the LAN, and the token is the check; nothing answers until config.toml enables
# them). A `listen` in config.toml still wins.
ENV XDG_CONFIG_HOME=/config SVOLDER_SOCKET=/run/svolder/svolder.sock \
    SVOLDER_WEB_LISTEN=0.0.0.0:8738 SVOLDER_MCP_LISTEN=0.0.0.0:8737
VOLUME ["/config", "/run/svolder"]
ENTRYPOINT ["docker-entrypoint"]
CMD []
