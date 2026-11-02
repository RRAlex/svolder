# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# The runtime image: the static binaries `make docker` builds into target/dist, the same bytes
# the .deb, the .rpm and the tarball carry, with no shared library to install beside them.
# The base image is pinned by digest, as the build container's is.
FROM ubuntu:26.04@sha256:f3d28607ddd78734bb7f71f117f3c6706c666b8b76cbff7c9ff6e5718d46ff64
# The uid the volumes are written as; `make docker` passes the host user's. The base image
# already carries a user at 1000, so whoever holds the requested uid is removed first.
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
COPY target/dist/svolder target/dist/svolderd /usr/local/bin/
COPY LICENSE target/dist/THIRD-PARTY-LICENSES /usr/share/licenses/svolder/
USER svolder
ENV XDG_CONFIG_HOME=/config SVOLDER_SOCKET=/run/svolder/svolder.sock
VOLUME ["/config", "/run/svolder"]
ENTRYPOINT ["svolderd"]
