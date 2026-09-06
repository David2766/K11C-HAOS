#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
CONNECTIVITY_DIR=$(cd -- "${SCRIPT_DIR}/.." && pwd)
APP_DIR=${1:-${CONNECTIVITY_DIR}/app}
RUN_SCRIPT="${APP_DIR}/rootfs/etc/services.d/k11c-connectivity/run"

fail() {
    echo "ERROR: $1" >&2
    exit 1
}

require_fixed_text() {
    local file="$1"
    local text="$2"

    grep -Fq -- "${text}" "${file}" || \
        fail "Required invariant is missing from ${file}: ${text}"
}

test -f "${APP_DIR}/config.yaml" || fail "config.yaml is missing"
test -f "${APP_DIR}/build.yaml" || fail "build.yaml is missing"
test -f "${APP_DIR}/Dockerfile" || fail "Dockerfile is missing"
test -f "${RUN_SCRIPT}" || fail "service run script is missing"

bash -n "${RUN_SCRIPT}"

require_fixed_text "${APP_DIR}/config.yaml" "kernel_modules: true"
require_fixed_text "${APP_DIR}/config.yaml" "devicetree: true"
require_fixed_text "${APP_DIR}/config.yaml" "host_network: true"
require_fixed_text "${APP_DIR}/config.yaml" "hassio_role: manager"
require_fixed_text "${APP_DIR}/config.yaml" "enabled: false"
require_fixed_text "${APP_DIR}/Dockerfile" \
    'ARG BUILD_FROM=ghcr.io/home-assistant/aarch64-base:3.24-2026.08.0'
require_fixed_text "${RUN_SCRIPT}" 'readonly EXPECTED_COMPATIBLE="kickpi,k11c"'
require_fixed_text "${RUN_SCRIPT}" 'test -r /device-tree/compatible'
require_fixed_text "${RUN_SCRIPT}" "tr '\\0' '\\n' </device-tree/compatible"
if grep -Fq -- '/device_tree/compatible' "${RUN_SCRIPT}"; then
    fail "Invalid Home Assistant device-tree mount path is present"
fi
require_fixed_text "${RUN_SCRIPT}" 'readonly EXPECTED_SDIO_ID="1ffe:6621"'
require_fixed_text "${RUN_SCRIPT}" 'test "$(uname -m)" = "aarch64"'
require_fixed_text "${RUN_SCRIPT}" 'module_dir="/opt/k11c/modules/${kernel_release}"'
require_fixed_text "${RUN_SCRIPT}" 'host_firmware_dir="/mnt/data/supervisor/apps/data/${app_slug}/firmware"'
require_fixed_text "${RUN_SCRIPT}" 'http://supervisor/apps/self/info'
require_fixed_text "${RUN_SCRIPT}" 'http://supervisor/addons/self/info'
require_fixed_text "${RUN_SCRIPT}" "wait_for_path '/sys/bus/platform/devices/sv6621s_wireless1*' 30 1"
require_fixed_text "${RUN_SCRIPT}" "wait_for_path '/sys/bus/platform/devices/btseekwave*' 30 1"
require_fixed_text "${RUN_SCRIPT}" "wait_for_path '/sys/class/net/*/wireless' 20 1"
require_fixed_text "${RUN_SCRIPT}" 'http://supervisor/network/reload'
require_fixed_text "${RUN_SCRIPT}" 'http://supervisor/network/info'
require_fixed_text "${RUN_SCRIPT}" '.interface == $interface and .type == "wireless"'
require_fixed_text "${RUN_SCRIPT}" 'refresh_supervisor_network "${wifi_interface}"'
require_fixed_text "${RUN_SCRIPT}" "wait_for_path '/sys/class/bluetooth/hci*' 20 1"
if grep -Eq -- 'network/interface/.*/update' "${RUN_SCRIPT}"; then
    fail "The app must not configure Wi-Fi credentials or IP settings"
fi
if grep -Fq -- 'sleep infinity' "${RUN_SCRIPT}"; then
    fail "BusyBox-incompatible sleep infinity is present"
fi

mapfile -t kernel_dirs < <(find "${APP_DIR}/modules" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)
test "${#kernel_dirs[@]}" -gt 0 || fail "No kernel module bundle exists"

for kernel_release in "${kernel_dirs[@]}"; do
    module_dir="${APP_DIR}/modules/${kernel_release}"
    mapfile -t module_names < <(find "${module_dir}" -maxdepth 1 -type f -name '*.ko' -printf '%f\n' | sort)
    expected_modules=(skw_sdio_lite.ko skwbt.ko swt6621s_wifi.ko)
    test "${module_names[*]}" = "${expected_modules[*]}" || \
        fail "Unexpected module set for ${kernel_release}: ${module_names[*]}"

    test -f "${module_dir}/modules.sha256" || fail "Module checksum list is missing"
    (cd "${module_dir}" && sha256sum --check --strict modules.sha256 >/dev/null) || \
        fail "Module checksum verification failed for ${kernel_release}"

    for module_file in "${module_dir}"/*.ko; do
        file "${module_file}" | grep -q 'ARM aarch64' || \
            fail "Module is not AArch64: ${module_file}"
        actual_release=$(modinfo -F vermagic "${module_file}" | awk '{print $1}')
        test "${actual_release}" = "${kernel_release}" || \
            fail "Module/kernel mismatch: ${module_file} has ${actual_release}"
        modinfo -F parm "${module_file}" | grep -q '^firmware_path:' || \
            fail "firmware_path parameter is missing: ${module_file}"
    done
done

firmware_dir="${APP_DIR}/firmware"
test -f "${firmware_dir}/firmware.sha256" || fail "Firmware checksum list is missing"
(cd "${firmware_dir}" && sha256sum --check --strict firmware.sha256 >/dev/null) || \
    fail "Firmware checksum verification failed"

expected_firmware=(
    SWT6621S_DRAM_SDIO.bin
    SWT6621S_IRAM_SDIO.bin
    SWT6621S_NV_SDIO_ALONE.bin
    SWT6621S_NV_SDIO_SHARE.bin
    SWT6621S_SEEKWAVE_R00000.bin
    SWT6621S_SEEKWAVE_R00001.bin
    SWT6621S_SEEKWAVE_R04000.bin
    SWT6621S_SEEKWAVE_R04001.bin
    sv6160.nvbin
    sv6160lite.nvbin
    sv6316.nvbin
)

for firmware_file in "${expected_firmware[@]}"; do
    test -f "${firmware_dir}/${firmware_file}" || \
        fail "Required firmware is missing: ${firmware_file}"
    grep -Eq "[[:space:]]+\\./${firmware_file//./\\.}$" "${firmware_dir}/firmware.sha256" || \
        fail "Firmware is absent from checksum list: ${firmware_file}"
done

base_line=$(grep -nF 'load_module skw_sdio_lite ' "${RUN_SCRIPT}" | cut -d: -f1)
wifi_platform_line=$(grep -nF "wait_for_path '/sys/bus/platform/devices/sv6621s_wireless1*'" \
    "${RUN_SCRIPT}" | cut -d: -f1)
bt_platform_line=$(grep -nF "wait_for_path '/sys/bus/platform/devices/btseekwave*'" \
    "${RUN_SCRIPT}" | cut -d: -f1)
wifi_line=$(grep -nF 'load_module swt6621s_wifi ' "${RUN_SCRIPT}" | cut -d: -f1)
wifi_ready_line=$(grep -nF "wait_for_path '/sys/class/net/*/wireless'" \
    "${RUN_SCRIPT}" | cut -d: -f1)
network_reload_line=$(grep -nF 'refresh_supervisor_network "${wifi_interface}"' \
    "${RUN_SCRIPT}" | cut -d: -f1)
bt_line=$(grep -nF 'load_module skwbt ' "${RUN_SCRIPT}" | cut -d: -f1)
(( base_line < wifi_platform_line && wifi_platform_line < wifi_line )) || \
    fail "Wi-Fi platform readiness is not checked between base and Wi-Fi module loading"
(( base_line < bt_platform_line && bt_platform_line < bt_line )) || \
    fail "Bluetooth platform readiness is not checked between base and Bluetooth module loading"
(( wifi_line < wifi_ready_line && wifi_ready_line < network_reload_line && network_reload_line < bt_line )) || \
    fail "Supervisor network refresh is not ordered after Wi-Fi readiness"

echo "K11C connectivity bundle verification passed"
