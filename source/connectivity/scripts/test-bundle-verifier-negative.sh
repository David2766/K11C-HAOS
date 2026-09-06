#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
set -Eeuo pipefail

if [[ ${1:-} == -h || ${1:-} == --help ]]; then
    echo "Usage: test-bundle-verifier-negative.sh"
    exit 0
fi

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
CONNECTIVITY_DIR=$(cd -- "${SCRIPT_DIR}/.." && pwd)
VERIFIER="${SCRIPT_DIR}/verify-connectivity-bundle.sh"
TEST_PARENT=${TMPDIR:-/tmp}
TEST_ROOT=$(mktemp -d "${TEST_PARENT}/k11c-bundle-negative.XXXXXX")

cleanup() {
    case "$(realpath "${TEST_ROOT}")" in
		"${TEST_PARENT}"/k11c-bundle-negative.*) rm -rf -- "${TEST_ROOT}" ;;
        *) echo "Refusing to remove unexpected test path: ${TEST_ROOT}" >&2 ;;
    esac
}
trap cleanup EXIT

fresh_fixture() {
    rm -rf -- "${TEST_ROOT}/app"
    cp -a "${CONNECTIVITY_DIR}/app" "${TEST_ROOT}/app"
}

expect_failure() {
    local name="$1"

    if "${VERIFIER}" "${TEST_ROOT}/app" >"${TEST_ROOT}/${name}.log" 2>&1; then
        echo "ERROR: negative test unexpectedly passed: ${name}" >&2
        exit 1
    fi

    echo "Expected rejection: ${name}"
}

fresh_fixture
mv "${TEST_ROOT}/app/modules/6.18.39-haos/skwbt.ko" \
    "${TEST_ROOT}/app/modules/6.18.39-haos/skwbt.ko.missing"
expect_failure missing_required_module

fresh_fixture
sed -i 's/readonly EXPECTED_SDIO_ID="1ffe:6621"/readonly EXPECTED_SDIO_ID="ffff:ffff"/' \
    "${TEST_ROOT}/app/rootfs/etc/services.d/k11c-connectivity/run"
expect_failure reversed_sdio_identity_guard

fresh_fixture
sed -i 's#/device-tree/compatible#/device_tree/compatible#g' \
    "${TEST_ROOT}/app/rootfs/etc/services.d/k11c-connectivity/run"
expect_failure invalid_devicetree_mount_path

fresh_fixture
sed -i 's/host_network: true/host_network: false/' \
    "${TEST_ROOT}/app/config.yaml"
expect_failure isolated_network_namespace

fresh_fixture
sed -i 's/hassio_role: manager/hassio_role: default/' \
    "${TEST_ROOT}/app/config.yaml"
expect_failure insufficient_supervisor_api_role

fresh_fixture
sed -i 's#http://supervisor/network/reload#http://supervisor/network/info#' \
    "${TEST_ROOT}/app/rootfs/etc/services.d/k11c-connectivity/run"
expect_failure missing_supervisor_network_reload

fresh_fixture
sed -i 's/\.type == "wireless"/.type == "ethernet"/' \
    "${TEST_ROOT}/app/rootfs/etc/services.d/k11c-connectivity/run"
expect_failure reversed_supervisor_wireless_guard

fresh_fixture
printf '\0' >>"${TEST_ROOT}/app/firmware/sv6160.nvbin"
expect_failure corrupted_nvbin_firmware

fresh_fixture
sed -i 's#sv6621s_wireless1\*#wrong_wireless_device*#' \
    "${TEST_ROOT}/app/rootfs/etc/services.d/k11c-connectivity/run"
expect_failure missing_wifi_platform_readiness_guard

echo "All negative bundle-verifier tests passed"
