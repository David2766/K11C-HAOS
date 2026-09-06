ARG BUILD_FROM=ghcr.io/home-assistant/aarch64-base:3.24-2026.08.0
FROM ${BUILD_FROM}

RUN apk add --no-cache curl jq kmod

COPY modules /opt/k11c/modules
COPY firmware /opt/k11c/firmware
COPY THIRD_PARTY_NOTICES.md /opt/k11c/THIRD_PARTY_NOTICES.md
COPY rootfs /

RUN chmod 0755 /etc/services.d/k11c-connectivity/run

LABEL \
    org.opencontainers.image.title="Home Assistant App: K11C Connectivity" \
    org.opencontainers.image.description="SeekWave VS/SWT6621S connectivity for KickPi K11C" \
    org.opencontainers.image.licenses="LicenseRef-Mixed"
