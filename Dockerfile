# Container image for the `forgedb` CLI — epic #181 / Docker channel.
#
# This does NOT recompile: it repackages the per-platform release tarball that
# release.yml (cargo-dist) already published to the GitHub Release, so the image
# is byte-identical to the native install and multi-arch is free (buildx sets
# TARGETARCH per platform). Ships BOTH `forgedb` and the bundled `forgedb-lsp`.
#
#   docker build --build-arg VERSION=0.1.0 -t forgedb .
#   docker run --rm -v "$PWD:/work" forgedb generate all --output ./generated
#
# The published multi-arch image is built by .github/workflows/docker.yml.

# ── fetch stage: download + extract the release tarball for this arch ──────────
FROM debian:bookworm-slim AS fetch
ARG VERSION
# Set by buildx per target platform: amd64 | arm64.
ARG TARGETARCH
ARG REPO=hoodiecollin/forgedb
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates curl xz-utils; \
    rm -rf /var/lib/apt/lists/*; \
    case "$TARGETARCH" in \
      amd64) triple=x86_64-unknown-linux-gnu ;; \
      arm64) triple=aarch64-unknown-linux-gnu ;; \
      *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    url="https://github.com/${REPO}/releases/download/v${VERSION}/forgedb-${triple}.tar.xz"; \
    echo "fetching $url"; \
    curl --proto '=https' --tlsv1.2 -fsSL "$url" -o /tmp/forgedb.tar.xz; \
    mkdir -p /tmp/x /out; \
    tar -xJf /tmp/forgedb.tar.xz -C /tmp/x; \
    find /tmp/x -type f -name forgedb     -exec install -m 0755 {} /out/forgedb \; ; \
    find /tmp/x -type f -name forgedb-lsp -exec install -m 0755 {} /out/forgedb-lsp \; ; \
    test -x /out/forgedb && test -x /out/forgedb-lsp

# ── runtime stage: slim image with just the binaries + CA certs ────────────────
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=fetch /out/forgedb     /usr/local/bin/forgedb
COPY --from=fetch /out/forgedb-lsp /usr/local/bin/forgedb-lsp
# Schemas/output get bind-mounted here.
WORKDIR /work
ENTRYPOINT ["forgedb"]
CMD ["--help"]
