#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
set -Eeuo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
CONNECTIVITY_DIR=$(cd -- "${SCRIPT_DIR}/.." && pwd)

HAOS_TREE=${HAOS_TREE:-}
VENDOR_INPUT=${VENDOR_TREE:-}
APP_DIR=${OUTPUT_ROOT:-}
WORK_ROOT=${WORK_ROOT:-${TMPDIR:-/tmp}}
FIRMWARE_MANIFEST="$CONNECTIVITY_DIR/firmware.sha256"
JOBS=${JOBS:-$(nproc)}
readonly EXPECTED_DRIVER_TREE_SHA256=14ae8c39dd2a23a9d7d6bccd797180dc93e0e3e3e9d16c619a74c627f76fc5a5

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

usage() {
	cat <<'EOF'
Usage: build-driver-bundle.sh --haos-tree PATH --vendor-tree PATH \
  --app-dir PATH [--work-root PATH] [--firmware-manifest FILE]

--haos-tree must be a configured HAOS build tree containing output/build and
output/host. --vendor-tree may point either to external/rkwifibt itself or to
its parent tree. The tested vendor firmware hashes are enforced before files
are copied into the local App directory.
EOF
}

while (($#)); do
	case "$1" in
		--haos-tree) (($# >= 2)) || fail "$1 requires a path"; HAOS_TREE=$2; shift 2 ;;
		--vendor-tree) (($# >= 2)) || fail "$1 requires a path"; VENDOR_INPUT=$2; shift 2 ;;
		--app-dir) (($# >= 2)) || fail "$1 requires a path"; APP_DIR=$2; shift 2 ;;
		--work-root) (($# >= 2)) || fail "$1 requires a path"; WORK_ROOT=$2; shift 2 ;;
		--firmware-manifest) (($# >= 2)) || fail "$1 requires a file"; FIRMWARE_MANIFEST=$2; shift 2 ;;
		-h|--help) usage; exit 0 ;;
		*) fail "unknown argument: $1" ;;
	esac
done

for command in awk file find git grep install make mktemp modinfo nproc patch \
	realpath sha256sum sort xargs; do
	command -v "$command" >/dev/null 2>&1 || fail "missing command: $command"
done
[[ "$JOBS" =~ ^[1-9][0-9]*$ ]] || fail "JOBS must be a positive integer"
[[ -n "$HAOS_TREE" ]] || fail "--haos-tree is required"
[[ -n "$VENDOR_INPUT" ]] || fail "--vendor-tree is required"
[[ -n "$APP_DIR" ]] || fail "--app-dir is required"

HAOS_TREE=$(realpath "$HAOS_TREE")
VENDOR_INPUT=$(realpath "$VENDOR_INPUT")
APP_DIR=$(realpath "$APP_DIR")
WORK_ROOT=$(realpath -m "$WORK_ROOT")
FIRMWARE_MANIFEST=$(realpath "$FIRMWARE_MANIFEST")

[[ -f "$APP_DIR/config.yaml" && -f "$APP_DIR/Dockerfile" ]] ||
	fail "--app-dir is not a K11C Connectivity App skeleton: $APP_DIR"
[[ -s "$FIRMWARE_MANIFEST" ]] || fail "firmware manifest is missing"

if [[ -d "$VENDOR_INPUT/drivers/skw6621s" && -d "$VENDOR_INPUT/firmware/seekwave" ]]; then
	VENDOR_TREE=$VENDOR_INPUT
elif [[ -d "$VENDOR_INPUT/external/rkwifibt/drivers/skw6621s" && \
	-d "$VENDOR_INPUT/external/rkwifibt/firmware/seekwave" ]]; then
	VENDOR_TREE="$VENDOR_INPUT/external/rkwifibt"
else
	fail "vendor tree must contain external/rkwifibt or be that directory"
fi

actual_driver_tree_sha256=$(
	cd "$VENDOR_TREE/drivers/skw6621s"
	find . -type f -print0 | sort -z | xargs -0 sha256sum | sha256sum | awk '{print $1}'
)
[[ "$actual_driver_tree_sha256" == "$EXPECTED_DRIVER_TREE_SHA256" ]] ||
	fail "unverified SeekWave driver source tree: expected $EXPECTED_DRIVER_TREE_SHA256, got $actual_driver_tree_sha256"

KERNEL_DIR=$(find "$HAOS_TREE/output/build" -maxdepth 1 -type d \
	-name 'linux-[0-9]*' -exec test -f '{}/.config' \; -print | sort -V | tail -n1)
[[ -n "$KERNEL_DIR" ]] || fail "configured HAOS kernel build tree not found"
[[ -f "$KERNEL_DIR/.config" ]] || fail "HAOS kernel tree is not configured"

TOOLCHAIN_PREFIX="$HAOS_TREE/output/host/bin/aarch64-buildroot-linux-gnu-"
[[ -x "${TOOLCHAIN_PREFIX}gcc" ]] || fail "HAOS AArch64 toolchain not found"
KERNEL_RELEASE=$(make -s -C "$KERNEL_DIR" ARCH=arm64 kernelrelease)

firmware_sources=(
	"firmware/seekwave/ea6x21qx/SWT6621S_DRAM_SDIO.bin"
	"firmware/seekwave/ea6x21qx/SWT6621S_IRAM_SDIO.bin"
	"firmware/seekwave/ea6x21qx/SWT6621S_NV_SDIO_ALONE.bin"
	"firmware/seekwave/ea6x21qx/SWT6621S_NV_SDIO_SHARE.bin"
	"firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R00000.bin"
	"firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R00001.bin"
	"firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R04000.bin"
	"firmware/seekwave/ea6x21qx/SWT6621S_SEEKWAVE_R04001.bin"
	"drivers/skw6621s/skwbt/sv6160.nvbin"
	"firmware/seekwave/ea6x21qx/sv6160lite.nvbin"
	"drivers/skw6621s/skwbt/sv6316.nvbin"
)

for relative_source in "${firmware_sources[@]}"; do
	source_file="$VENDOR_TREE/$relative_source"
	filename=$(basename "$source_file")
	[[ -s "$source_file" ]] || fail "vendor firmware is missing: $relative_source"
	expected=$(awk -v name="./$filename" '$2 == name {print $1}' "$FIRMWARE_MANIFEST")
	[[ "$expected" =~ ^[0-9a-f]{64}$ ]] ||
		fail "tested hash is missing for firmware: $filename"
	actual=$(sha256sum "$source_file" | awk '{print $1}')
	[[ "$actual" == "$expected" ]] ||
		fail "unverified firmware $filename: expected $expected, got $actual"
done

mkdir -p "$WORK_ROOT"
BUILD_DIR=$(mktemp -d "$WORK_ROOT/k11c-skw-${KERNEL_RELEASE}.XXXXXX")
SOURCE_DIR="$BUILD_DIR/skw6621s"

cleanup() {
	case "$(realpath -m "$BUILD_DIR")" in
		"$WORK_ROOT"/k11c-skw-*) rm -rf -- "$BUILD_DIR" ;;
		*) echo "Refusing to remove unexpected build path: $BUILD_DIR" >&2 ;;
	esac
}
trap cleanup EXIT

cp -a "$VENDOR_TREE/drivers/skw6621s" "$SOURCE_DIR"
cd "$SOURCE_DIR"
for patch_file in "$CONNECTIVITY_DIR"/driver-patches/000[1-5]-*.patch; do
	patch --batch --forward -p1 <"$patch_file"
done
git apply "$CONNECTIVITY_DIR/driver-patches/0006-app-owned-firmware-directory.patch"

make -C "$KERNEL_DIR" \
	M="$SOURCE_DIR" \
	ARCH=arm64 \
	CROSS_COMPILE="$TOOLCHAIN_PREFIX" \
	CONFIG_SKW_USB=n \
	CONFIG_WLAN_VENDOR_SWT6621U=n \
	CONFIG_SKW6621S=y \
	CONFIG_SKW_BT=m \
	-j"$JOBS" modules

MODULE_OUTPUT="$APP_DIR/modules/$KERNEL_RELEASE"
FIRMWARE_OUTPUT="$APP_DIR/firmware"
case "$(realpath -m "$MODULE_OUTPUT")" in
	"$APP_DIR"/modules/*) ;;
	*) fail "refusing unexpected module output path: $MODULE_OUTPUT" ;;
esac
case "$(realpath -m "$FIRMWARE_OUTPUT")" in
	"$APP_DIR"/firmware) ;;
	*) fail "refusing unexpected firmware output path: $FIRMWARE_OUTPUT" ;;
esac
rm -rf -- "$MODULE_OUTPUT" "$FIRMWARE_OUTPUT"
install -d "$MODULE_OUTPUT" "$FIRMWARE_OUTPUT"

install -m 0644 "$SOURCE_DIR/seekwaveplatform_lite/skw_sdio_lite.ko" "$MODULE_OUTPUT/"
install -m 0644 "$SOURCE_DIR/swt6621s_wifi/swt6621s_wifi.ko" "$MODULE_OUTPUT/"
install -m 0644 "$SOURCE_DIR/skwbt/skwbt.ko" "$MODULE_OUTPUT/"
"${TOOLCHAIN_PREFIX}strip" --strip-debug "$MODULE_OUTPUT"/*.ko

for relative_source in "${firmware_sources[@]}"; do
	install -m 0644 "$VENDOR_TREE/$relative_source" "$FIRMWARE_OUTPUT/"
done
install -m 0644 "$FIRMWARE_MANIFEST" "$FIRMWARE_OUTPUT/firmware.sha256"

for module in skw_sdio_lite swt6621s_wifi skwbt; do
	module_file="$MODULE_OUTPUT/$module.ko"
	file "$module_file" | grep -q 'ARM aarch64' || fail "module is not AArch64: $module"
	actual_release=$(modinfo -F vermagic "$module_file" | awk '{print $1}')
	[[ "$actual_release" == "$KERNEL_RELEASE" ]] ||
		fail "module/kernel mismatch: $module has $actual_release"
	modinfo -F parm "$module_file" | grep -q '^firmware_path:' ||
		fail "firmware_path parameter is missing: $module"
done
[[ ! -e "$MODULE_OUTPUT/skw_usb_lite.ko" ]] || fail "USB transport module was built"
[[ ! -e "$MODULE_OUTPUT/swt6621u_wifi.ko" ]] || fail "USB Wi-Fi module was built"

(
	cd "$MODULE_OUTPUT"
	sha256sum ./*.ko >modules.sha256
)
(
	cd "$FIRMWARE_OUTPUT"
	sha256sum --check --strict firmware.sha256 >/dev/null
)
"$SCRIPT_DIR/verify-connectivity-bundle.sh" "$APP_DIR"

echo "PASS: built K11C Connectivity App payload for $KERNEL_RELEASE"
echo "App directory: $APP_DIR"
