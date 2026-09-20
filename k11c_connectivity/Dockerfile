ARG BUILD_FROM=ghcr.io/home-assistant/aarch64-base:3.24-2026.08.0
FROM ghcr.io/blakeblackshear/frigate@sha256:c707dfa94a2efc9e994165cc7968517ebc02edd5b391789e20f6ace006bf29e3 AS inference-runtime
COPY inference-assets/librknnrt.so /input/librknnrt.so
COPY parser-assets/python_multipart-0.0.32-py3-none-any.whl /input/upload-parser.whl
COPY build/collect-runtime.py /collect-runtime.py
RUN python3 /collect-runtime.py

FROM ${BUILD_FROM}
ARG BUILD_VERSION=0.5.3

RUN apk add --no-cache curl jq kmod
COPY --from=inference-runtime /runtime /opt/k11c/npu-runtime
COPY inference-assets/librknnrt.so /usr/lib/librknnrt.so
COPY inference-assets/model.rknn /opt/k11c/inference/model.rknn
COPY inference-assets/notices /opt/k11c/inference/notices

COPY modules /opt/k11c/modules
COPY firmware /opt/k11c/firmware
COPY THIRD_PARTY_NOTICES.md /opt/k11c/THIRD_PARTY_NOTICES.md
COPY ffmpeg-assets /opt/k11c/ffmpeg-assets
COPY rootfs /

RUN chmod 0755 /etc/services.d/k11c-connectivity/run \
    /etc/services.d/k11c-npu/run /usr/local/bin/k11c-npu-load \
    /usr/local/bin/k11c-api-python /etc/services.d/k11c-inference/run \
    /etc/services.d/k11c-frigate-native/run

LABEL \
    io.hass.version="${BUILD_VERSION}" \
    io.hass.type="app" \
    io.hass.arch="aarch64" \
    org.opencontainers.image.title="Home Assistant App: K11C Connectivity" \
    org.opencontainers.image.description="K11C Wi-Fi, Bluetooth, and optional NPU detection API" \
    org.opencontainers.image.licenses="LicenseRef-Mixed"
