#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
set -Eeuo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
PORT_ROOT=$(cd -- "${SCRIPT_DIR}/.." && pwd)

UBOOT_TREE=""
DDR_BLOB=""
BL31_BLOB=""
OUTPUT=""
WORK_ROOT=${WORK_ROOT:-${TMPDIR:-/tmp}}
JOBS=${JOBS:-$(nproc)}

readonly EXPECTED_UBOOT_COMMIT=88dc2788777babfd6322fa655df549a019aa1e69
readonly EXPECTED_DDR_SHA256=20e4bb076847bd019fcdeb7bdc15bd249890f07ecc76e9937101f22e50950982
readonly EXPECTED_BL31_SHA256=65110f822fdbdd0163ce2dabc60591e7a8a0ffbc9471780e29eef0062f9ed7b6
readonly BOOTCOMMAND='fdt addr ${fdtcontroladdr}; fdt set /mmc@fe2c0000 status okay; bootflow scan -lbG mmc1; bootflow scan -lbG mmc0'

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

usage() {
	cat <<'EOF'
Usage: build-k11c-uboot.sh --uboot-tree PATH --ddr-blob FILE \
  --bl31 FILE --output FILE

The U-Boot tree must be a clean checkout of upstream tag v2026.04 at commit
88dc2788777babfd6322fa655df549a019aa1e69. The tested Rockchip DDR and BL31
binary hashes are enforced. The source checkout is never modified.
EOF
}

require_command() {
	command -v "$1" >/dev/null 2>&1 || fail "missing command: $1"
}

require_file() {
	[[ -s "$1" ]] || fail "required file is missing or empty: $1"
}

require_hash() {
	local file=$1 expected=$2 actual
	actual=$(sha256sum "$file" | awk '{print $1}')
	[[ "$actual" == "$expected" ]] ||
		fail "SHA-256 mismatch for $file: expected $expected, got $actual"
}

expect_eq() {
	[[ "$1" == "$2" ]] || fail "$3: expected '$2', got '$1'"
}

expect_absent() {
	local dtb=$1 node=$2 property=$3
	if fdtget "$dtb" "$node" "$property" >/dev/null 2>&1; then
		fail "$node/$property must be absent"
	fi
}

while (($#)); do
	case "$1" in
		--uboot-tree) (($# >= 2)) || fail "$1 requires a path"; UBOOT_TREE=$2; shift 2 ;;
		--ddr-blob) (($# >= 2)) || fail "$1 requires a file"; DDR_BLOB=$2; shift 2 ;;
		--bl31) (($# >= 2)) || fail "$1 requires a file"; BL31_BLOB=$2; shift 2 ;;
		--output) (($# >= 2)) || fail "$1 requires a file"; OUTPUT=$2; shift 2 ;;
		-h|--help) usage; exit 0 ;;
		*) fail "unknown argument: $1" ;;
	esac
done

for command in awk file fdtget fdtput git grep install make mktemp nproc \
	realpath sha256sum; do
	require_command "$command"
done
[[ "$JOBS" =~ ^[1-9][0-9]*$ ]] || fail "JOBS must be a positive integer"
[[ -n "$UBOOT_TREE" ]] || fail "--uboot-tree is required"
[[ -n "$DDR_BLOB" ]] || fail "--ddr-blob is required"
[[ -n "$BL31_BLOB" ]] || fail "--bl31 is required"
[[ -n "$OUTPUT" ]] || fail "--output is required"

UBOOT_TREE=$(realpath "$UBOOT_TREE")
DDR_BLOB=$(realpath "$DDR_BLOB")
BL31_BLOB=$(realpath "$BL31_BLOB")
OUTPUT=$(realpath -m "$OUTPUT")
WORK_ROOT=$(realpath -m "$WORK_ROOT")

require_file "$DDR_BLOB"
require_file "$BL31_BLOB"
require_file "$PORT_ROOT/device-tree/linux/rk3566-kickpi-k11c.dts"
require_file "$PORT_ROOT/device-tree/u-boot/rk3566-kickpi-k11c-u-boot.dtsi"
require_file "$PORT_ROOT/u-boot/board/hardkernel/odroid_m1s/Makefile"
require_file "$PORT_ROOT/u-boot/board/hardkernel/odroid_m1s/k11c.c"
require_hash "$DDR_BLOB" "$EXPECTED_DDR_SHA256"
require_hash "$BL31_BLOB" "$EXPECTED_BL31_SHA256"

git -C "$UBOOT_TREE" rev-parse --is-inside-work-tree >/dev/null 2>&1 ||
	fail "--uboot-tree is not a Git checkout"
actual_commit=$(git -C "$UBOOT_TREE" rev-parse HEAD)
expect_eq "$actual_commit" "$EXPECTED_UBOOT_COMMIT" "U-Boot source commit"
[[ -z "$(git -C "$UBOOT_TREE" status --porcelain --untracked-files=all)" ]] ||
	fail "U-Boot source checkout must be clean"

for artifact in "$OUTPUT" "$OUTPUT.manifest.txt" "$OUTPUT.control.dtb" \
	"$OUTPUT.config"; do
	[[ ! -e "$artifact" ]] || fail "output already exists: $artifact"
done
mkdir -p "$(dirname "$OUTPUT")" "$WORK_ROOT"

BUILD_ROOT=$(mktemp -d "$WORK_ROOT/k11c-uboot.XXXXXX")
SOURCE_TREE="$BUILD_ROOT/source"
BUILD_DIR="$BUILD_ROOT/build"
WORKTREE_ADDED=false

cleanup() {
	if [[ "$WORKTREE_ADDED" == true ]]; then
		git -C "$UBOOT_TREE" worktree remove --force "$SOURCE_TREE" >/dev/null 2>&1 || true
	fi
	case "$(realpath -m "$BUILD_ROOT")" in
		"$WORK_ROOT"/k11c-uboot.*) rm -rf -- "$BUILD_ROOT" ;;
		*) echo "Refusing to remove unexpected build path: $BUILD_ROOT" >&2 ;;
	esac
}
trap cleanup EXIT

git -C "$UBOOT_TREE" worktree add --detach "$SOURCE_TREE" "$EXPECTED_UBOOT_COMMIT" \
	>/dev/null
WORKTREE_ADDED=true

install -m 0644 "$PORT_ROOT/device-tree/linux/rk3566-kickpi-k11c.dts" \
	"$SOURCE_TREE/dts/upstream/src/arm64/rockchip/rk3566-kickpi-k11c.dts"
install -m 0644 "$PORT_ROOT/device-tree/u-boot/rk3566-kickpi-k11c-u-boot.dtsi" \
	"$SOURCE_TREE/arch/arm/dts/rk3566-kickpi-k11c-u-boot.dtsi"
install -m 0644 "$PORT_ROOT/u-boot/board/hardkernel/odroid_m1s/Makefile" \
	"$SOURCE_TREE/board/hardkernel/odroid_m1s/Makefile"
install -m 0644 "$PORT_ROOT/u-boot/board/hardkernel/odroid_m1s/k11c.c" \
	"$SOURCE_TREE/board/hardkernel/odroid_m1s/k11c.c"

export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-1788537600}
export KBUILD_BUILD_TIMESTAMP=${KBUILD_BUILD_TIMESTAMP:-'2026-09-05 00:00:00 +0800'}
export KBUILD_BUILD_USER=${KBUILD_BUILD_USER:-k11c}
export KBUILD_BUILD_HOST=${KBUILD_BUILD_HOST:-builder}
export KBUILD_BUILD_VERSION=${KBUILD_BUILD_VERSION:-1}

make -C "$SOURCE_TREE" O="$BUILD_DIR" CROSS_COMPILE=aarch64-linux-gnu- \
	odroid-m1s-rk3566_defconfig >"$BUILD_ROOT/config.log" 2>&1
"$SOURCE_TREE/scripts/config" --file "$BUILD_DIR/.config" \
	--set-str DEFAULT_DEVICE_TREE "rockchip/rk3566-kickpi-k11c"
"$SOURCE_TREE/scripts/config" --file "$BUILD_DIR/.config" \
	--set-str OF_LIST "rockchip/rk3566-kickpi-k11c"
"$SOURCE_TREE/scripts/config" --file "$BUILD_DIR/.config" \
	--set-str SPL_OF_LIST "rockchip/rk3566-kickpi-k11c"
"$SOURCE_TREE/scripts/config" --file "$BUILD_DIR/.config" --disable ENV_IS_IN_MMC
"$SOURCE_TREE/scripts/config" --file "$BUILD_DIR/.config" --enable ENV_IS_NOWHERE
"$SOURCE_TREE/scripts/config" --file "$BUILD_DIR/.config" \
	--set-str BOOTCOMMAND "$BOOTCOMMAND"
make -C "$SOURCE_TREE" O="$BUILD_DIR" CROSS_COMPILE=aarch64-linux-gnu- \
	olddefconfig >>"$BUILD_ROOT/config.log" 2>&1

if ! make -C "$SOURCE_TREE" O="$BUILD_DIR" CROSS_COMPILE=aarch64-linux-gnu- \
	BL31="$BL31_BLOB" ROCKCHIP_TPL="$DDR_BLOB" -j"$JOBS" all \
	>"$BUILD_ROOT/build.log" 2>&1; then
	tail -100 "$BUILD_ROOT/build.log" >&2
	fail "U-Boot build failed"
fi

for file in "$BUILD_DIR/u-boot" "$BUILD_DIR/u-boot.dtb" \
	"$BUILD_DIR/spl/u-boot-spl.dtb" "$BUILD_DIR/u-boot-rockchip.bin"; do
	require_file "$file"
done

assert_wireless_dtb() {
	local dtb=$1 wifi_enable sclktx lrcktx sdi sdo actual
	expect_eq "$(fdtget "$dtb" / compatible | awk '{print $1}')" \
		"kickpi,k11c" "board compatible"
	expect_eq "$(fdtget "$dtb" /mmc@fe2c0000 status)" disabled \
		"U-Boot SDIO probe suppression"
	expect_absent "$dtb" /mmc@fe2c0000 vqmmc-supply
	expect_eq "$(fdtget "$dtb" /i2s@fe420000 status)" disabled \
		"I2S2 controller status"
	wifi_enable=$(fdtget -t x "$dtb" /pinctrl/sdio-pwrseq/wifi-enable-h phandle)
	sclktx=$(fdtget -t x "$dtb" /pinctrl/i2s2/i2s2m0-sclktx phandle)
	lrcktx=$(fdtget -t x "$dtb" /pinctrl/i2s2/i2s2m0-lrcktx phandle)
	sdi=$(fdtget -t x "$dtb" /pinctrl/i2s2/i2s2m0-sdi phandle)
	sdo=$(fdtget -t x "$dtb" /pinctrl/i2s2/i2s2m0-sdo phandle)
	actual=$(fdtget -t x "$dtb" /sdio-pwrseq pinctrl-0)
	expect_eq "$actual" "$wifi_enable $sclktx $lrcktx $sdi $sdo" \
		"wireless reset and PCM pinctrl sequence"
}

assert_lan_dtb() {
	local dtb=$1 property properties
	expect_eq "$(fdtget "$dtb" /ethernet@fe010000 status)" okay \
		"Ethernet status"
	expect_eq "$(fdtget "$dtb" /ethernet@fe010000 phy-mode)" rgmii \
		"Ethernet PHY mode"
	expect_eq "$(fdtget "$dtb" /ethernet@fe010000 tx_delay)" 66 \
		"Ethernet TX delay"
	expect_eq "$(fdtget "$dtb" /ethernet@fe010000 rx_delay)" 57 \
		"Ethernet RX delay"
	properties=$(fdtget -p "$dtb" /ethernet@fe010000/mdio/ethernet-phy@0)
	for property in reset-gpios eee-broken-100tx eee-broken-1000t; do
		grep -Fxq "$property" <<<"$properties" ||
			fail "Ethernet PHY property is missing: $property"
	done
}

assert_wireless_dtb "$BUILD_DIR/u-boot.dtb"
assert_lan_dtb "$BUILD_DIR/u-boot.dtb"
grep -Fqx "CONFIG_BOOTCOMMAND=\"$BOOTCOMMAND\"" "$BUILD_DIR/.config" ||
	fail "compiled boot order changed"
grep -Fqx 'CONFIG_ENV_IS_NOWHERE=y' "$BUILD_DIR/.config" ||
	fail "environment storage policy changed"
grep -aFq 'K11C: Maxio PHY %08x prepared' "$BUILD_DIR/u-boot" ||
	fail "K11C Maxio hook is absent from U-Boot"

# Prove that the validators reject the two board-critical omissions.
cp "$BUILD_DIR/u-boot.dtb" "$BUILD_ROOT/no-pcm.dtb"
fdtput -d "$BUILD_ROOT/no-pcm.dtb" /sdio-pwrseq pinctrl-0
if (assert_wireless_dtb "$BUILD_ROOT/no-pcm.dtb") >/dev/null 2>&1; then
	fail "wireless pinctrl negative mutation was accepted"
fi
cp "$BUILD_DIR/u-boot.dtb" "$BUILD_ROOT/no-eee.dtb"
fdtput -d "$BUILD_ROOT/no-eee.dtb" \
	/ethernet@fe010000/mdio/ethernet-phy@0 eee-broken-1000t
if (assert_lan_dtb "$BUILD_ROOT/no-eee.dtb") >/dev/null 2>&1; then
	fail "Ethernet EEE negative mutation was accepted"
fi

install -m 0644 "$BUILD_DIR/u-boot-rockchip.bin" "$OUTPUT"
install -m 0644 "$BUILD_DIR/u-boot.dtb" "$OUTPUT.control.dtb"
install -m 0644 "$BUILD_DIR/.config" "$OUTPUT.config"

output_sha256=$(sha256sum "$OUTPUT" | awk '{print $1}')
{
	printf 'uboot.version=2026.04\n'
	printf 'uboot.commit=%s\n' "$EXPECTED_UBOOT_COMMIT"
	printf 'uboot.sha256=%s\n' "$output_sha256"
	printf 'uboot.control_dtb.sha256=%s\n' \
		"$(sha256sum "$OUTPUT.control.dtb" | awk '{print $1}')"
	printf 'rockchip.ddr.sha256=%s\n' "$EXPECTED_DDR_SHA256"
	printf 'rockchip.bl31.sha256=%s\n' "$EXPECTED_BL31_SHA256"
	printf 'linux.dts.sha256=%s\n' \
		"$(sha256sum "$PORT_ROOT/device-tree/linux/rk3566-kickpi-k11c.dts" | awk '{print $1}')"
	printf 'uboot.dtsi.sha256=%s\n' \
		"$(sha256sum "$PORT_ROOT/device-tree/u-boot/rk3566-kickpi-k11c-u-boot.dtsi" | awk '{print $1}')"
	printf 'boot_order=microSD-then-eMMC\n'
	printf 'sdio_vqmmc_supply_present=false\n'
	printf 'wifi_pcm_pinctrl_present=true\n'
	printf 'ethernet_eee_quirks_present=true\n'
	printf 'negative_tests=pass\n'
} >"$OUTPUT.manifest.txt"

[[ -z "$(git -C "$UBOOT_TREE" status --porcelain --untracked-files=all)" ]] ||
	fail "U-Boot source checkout changed during the build"

echo "PASS: built $OUTPUT"
echo "SHA256: $output_sha256"
echo "Manifest: $OUTPUT.manifest.txt"
