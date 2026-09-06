#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
CONNECTIVITY_DIR=$(cd -- "${SCRIPT_DIR}/.." && pwd)

HAOS_TREE=${HAOS_TREE:-/home/user/work/haos-18.2}
VENDOR_TREE=${VENDOR_TREE:-/mnt/c/rom/HAOS/vendor-k11c/kernel-6.1-20260609/external/rkwifibt}
WORK_ROOT=${WORK_ROOT:-/home/user/work/k11c-port-build}
OUTPUT_ROOT=${OUTPUT_ROOT:-${CONNECTIVITY_DIR}/app}

KERNEL_DIR=$(find "${HAOS_TREE}/output/build" -maxdepth 1 -type d \
    -name 'linux-[0-9]*' -exec test -f '{}/.config' \; -print | sort -V | tail -n1)
test -n "${KERNEL_DIR}" || { echo "HAOS kernel build tree not found" >&2; exit 1; }
test -f "${KERNEL_DIR}/.config" || { echo "Kernel tree is not configured" >&2; exit 1; }

TOOLCHAIN_PREFIX="${HAOS_TREE}/output/host/bin/aarch64-buildroot-linux-gnu-"
test -x "${TOOLCHAIN_PREFIX}gcc" || { echo "HAOS aarch64 toolchain not found" >&2; exit 1; }

KERNEL_RELEASE=$(make -s -C "${KERNEL_DIR}" ARCH=arm64 kernelrelease)
BUILD_DIR=$(mktemp -d "${WORK_ROOT}/skw-${KERNEL_RELEASE}.XXXXXX")
SOURCE_DIR="${BUILD_DIR}/skw6621s"

cleanup() {
    case "$(realpath "${BUILD_DIR}")" in
        "$(realpath "${WORK_ROOT}")"/skw-*) rm -rf -- "${BUILD_DIR}" ;;
        *) echo "Refusing to remove unexpected build path: ${BUILD_DIR}" >&2 ;;
    esac
}
trap cleanup EXIT

cp -a "${VENDOR_TREE}/drivers/skw6621s" "${SOURCE_DIR}"

cd "${SOURCE_DIR}"
patch --batch --forward -p1 <"${CONNECTIVITY_DIR}/driver-patches/0001-linux-6.13-build-compat.patch"
patch --batch --forward -p1 <"${CONNECTIVITY_DIR}/driver-patches/0002-linux-6.18-cfg80211-compat.patch"
patch --batch --forward -p1 <"${CONNECTIVITY_DIR}/driver-patches/0003-linux-6.18-timer-and-rockchip-portability.patch"
patch --batch --forward -p1 <"${CONNECTIVITY_DIR}/driver-patches/0004-linux-6.18-power-and-platform-api.patch"
patch --batch --forward -p1 <"${CONNECTIVITY_DIR}/driver-patches/0005-linux-6.18-bluetooth-remove-and-safe-version-log.patch"
git apply "${CONNECTIVITY_DIR}/driver-patches/0006-app-owned-firmware-directory.patch"

make -C "${KERNEL_DIR}" \
    M="${SOURCE_DIR}" \
    ARCH=arm64 \
    CROSS_COMPILE="${TOOLCHAIN_PREFIX}" \
    CONFIG_SKW_USB=n \
    CONFIG_WLAN_VENDOR_SWT6621U=n \
    CONFIG_SKW6621S=y \
    CONFIG_SKW_BT=m \
    -j"$(nproc)" modules

MODULE_OUTPUT="${OUTPUT_ROOT}/modules/${KERNEL_RELEASE}"
FIRMWARE_OUTPUT="${OUTPUT_ROOT}/firmware"
case "$(realpath -m "${MODULE_OUTPUT}")" in
    "$(realpath "${OUTPUT_ROOT}")"/modules/*) ;;
    *) echo "Refusing unexpected module output path: ${MODULE_OUTPUT}" >&2; exit 1 ;;
esac
case "$(realpath -m "${FIRMWARE_OUTPUT}")" in
    "$(realpath "${OUTPUT_ROOT}")"/firmware) ;;
    *) echo "Refusing unexpected firmware output path: ${FIRMWARE_OUTPUT}" >&2; exit 1 ;;
esac
rm -rf -- "${MODULE_OUTPUT}" "${FIRMWARE_OUTPUT}"
install -d "${MODULE_OUTPUT}" "${FIRMWARE_OUTPUT}"

install -m 0644 "${SOURCE_DIR}/seekwaveplatform_lite/skw_sdio_lite.ko" "${MODULE_OUTPUT}/"
install -m 0644 "${SOURCE_DIR}/swt6621s_wifi/swt6621s_wifi.ko" "${MODULE_OUTPUT}/"
install -m 0644 "${SOURCE_DIR}/skwbt/skwbt.ko" "${MODULE_OUTPUT}/"
"${TOOLCHAIN_PREFIX}strip" --strip-debug "${MODULE_OUTPUT}"/*.ko

install -m 0644 "${VENDOR_TREE}/firmware/seekwave/ea6x21qx/SWT6621S_IRAM_SDIO.bin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/firmware/seekwave/ea6x21qx/SWT6621S_DRAM_SDIO.bin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/firmware/seekwave/ea6x21qx/SWT6621S_NV_SDIO_ALONE.bin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/firmware/seekwave/ea6x21qx/SWT6621S_NV_SDIO_SHARE.bin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R00000.bin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R00001.bin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R04000.bin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R04001.bin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/firmware/seekwave/ea6x21qx/sv6160lite.nvbin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/drivers/skw6621s/skwbt/sv6160.nvbin" "${FIRMWARE_OUTPUT}/"
install -m 0644 "${VENDOR_TREE}/drivers/skw6621s/skwbt/sv6316.nvbin" "${FIRMWARE_OUTPUT}/"

for module in skw_sdio_lite swt6621s_wifi skwbt; do
    module_file="${MODULE_OUTPUT}/${module}.ko"
    file "${module_file}" | grep -q 'ARM aarch64'
    test "$(modinfo -F vermagic "${module_file}" | awk '{print $1}')" = "${KERNEL_RELEASE}"
    modinfo -F parm "${module_file}" | grep -q '^firmware_path:'
done

test ! -e "${MODULE_OUTPUT}/skw_usb_lite.ko"
test ! -e "${MODULE_OUTPUT}/swt6621u_wifi.ko"

cd "${MODULE_OUTPUT}"
sha256sum ./*.ko >modules.sha256
cd "${FIRMWARE_OUTPUT}"
sha256sum ./*.bin ./*.nvbin >firmware.sha256

echo "Built K11C connectivity bundle for ${KERNEL_RELEASE}"
