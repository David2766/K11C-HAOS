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
# Keep the U-Boot OTP device in its already-created live tree. Only suppress
# that duplicate in the flat DT handed to Linux; the r22 vendor OTP stays on.
readonly BOOTCOMMAND='fdt addr ${fdtcontroladdr}; fdt set /mmc@fe2c0000 status okay; fdt set /nvmem@fe38c000 status disabled; bootflow scan -lbG mmc1; bootflow scan -lbG mmc0'

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

assert_ref_list() {
	local dtb=$1 node=$2 property=$3 expected="" target
	shift 3
	for target in "$@"; do
		expected+="$(fdtget -t x "$dtb" "$target" phandle) "
	done
	expect_eq "$(fdtget -t x "$dtb" "$node" "$property")" "${expected% }" \
		"$node/$property references"
}

assert_peripherals_dtb() {
	local dtb=$1 node pmic=/i2c@fdd40000/pmic@20 gpio3 gpio4 cru
	for node in /hdmi-sound /hdmi@fe0a0000 /i2s@fe400000 /i2s@fe410000 \
		/i2c@fe5c0000 /serial@fe6b0000 /serial@fe6d0000; do
		expect_eq "$(fdtget "$dtb" "$node" status)" okay "$node status"
	done
	assert_ref_list "$dtb" /hdmi-sound/simple-audio-card,cpu sound-dai /i2s@fe400000
	assert_ref_list "$dtb" /rk809-sound/simple-audio-card,cpu sound-dai /i2s@fe410000
	assert_ref_list "$dtb" /rk809-sound/simple-audio-card,codec sound-dai "$pmic"
	expect_eq "$(fdtget "$dtb" /rk809-sound compatible)" simple-audio-card "analog card binding"
	expect_eq "$(fdtget "$dtb" /rk809-sound simple-audio-card,format)" i2s "analog format"
	expect_eq "$(fdtget "$dtb" /rk809-sound simple-audio-card,mclk-fs)" 256 "analog MCLK ratio"
	expect_eq "$(fdtget "$dtb" /rk809-sound/simple-audio-card,codec system-clock-frequency)" \
		12288000 "analog fixed 48 kHz system clock"
	fdtget "$dtb" /rk809-sound/simple-audio-card,codec system-clock-fixed >/dev/null || \
		fail "analog fixed-clock flag missing"
	# A child clocks property takes precedence over system-clock-frequency in
	# simple_util_parse_clk(). Keep the fixed-rate constraint on this exact DAI.
	expect_absent "$dtb" /rk809-sound/simple-audio-card,codec clocks
	expect_eq "$(fdtget "$dtb" "$pmic" '#sound-dai-cells')" 0 "mainline RK809 DAI cells"
	expect_eq "$(fdtget "$dtb" "$pmic" clock-names)" mclk "codec clock name on PMIC"
	cru=$(fdtget -t x "$dtb" /clock-controller@fdd20000 phandle)
	# rk3568-cru.h: I2S1_MCLKOUT_TX=72; CLK_I2S1_8CH_TX=406.
	expect_eq "$(fdtget -t x "$dtb" "$pmic" clocks)" "$cru 48" "codec MCLK"
	expect_eq "$(fdtget -t x "$dtb" "$pmic" assigned-clocks)" "$cru 48" "codec clock mux"
	expect_eq "$(fdtget -t x "$dtb" "$pmic" assigned-clock-parents)" "$cru 196" "codec clock parent"
	fdtget "$dtb" "$pmic/codec" rockchip,mic-in-differential >/dev/null || fail "differential microphone flag missing"
	fdtget "$dtb" /i2s@fe410000 rockchip,trcm-sync-tx-only >/dev/null || fail "I2S1 TX clock synchronization missing"
	assert_ref_list "$dtb" "$pmic" pinctrl-0 /pinctrl/pmic/pmic-int /pinctrl/i2s1/i2s1m0-mclk
	assert_ref_list "$dtb" /i2s@fe410000 pinctrl-0 /pinctrl/i2s1/i2s1m0-sclktx \
		/pinctrl/i2s1/i2s1m0-lrcktx /pinctrl/i2s1/i2s1m0-sdi0 /pinctrl/i2s1/i2s1m0-sdo0
	assert_ref_list "$dtb" /rk809-sound simple-audio-card,aux-devs /audio-amplifier
	assert_ref_list "$dtb" /audio-amplifier VCC-supply /regulator-5v0-sys
	assert_ref_list "$dtb" /audio-amplifier pinctrl-0 /pinctrl/audio/speaker-enable
	assert_ref_list "$dtb" /rk809-sound pinctrl-0 /pinctrl/audio/hp-det
	expect_eq "$(fdtget "$dtb" /audio-amplifier compatible)" simple-audio-amplifier "speaker driver"
	expect_eq "$(fdtget "$dtb" /audio-amplifier sound-name-prefix)" 'Speaker Amp' "speaker route prefix"
	gpio3=$(fdtget -t x "$dtb" /pinctrl/gpio@fe760000 phandle)
	gpio4=$(fdtget -t x "$dtb" /pinctrl/gpio@fe770000 phandle)
	expect_eq "$(fdtget -t x "$dtb" /audio-amplifier enable-gpios)" "$gpio3 11 0" "HT6872 active-high CTRL"
	expect_eq "$(fdtget -t x "$dtb" /rk809-sound simple-audio-card,hp-det-gpios)" "$gpio4 16 0" "stock jack detect"
	expect_eq "$(fdtget "$dtb" /rk809-sound simple-audio-card,routing)" \
		'Headphones HPOL Headphones HPOR Speaker Amp INL SPKO Speaker Speaker Amp OUTL MICL Mic Jack' "analog signal routes"
	assert_ref_list "$dtb" /i2c@fe5c0000 pinctrl-0 /pinctrl/i2c3/i2c3m0-xfer
	assert_ref_list "$dtb" /serial@fe6b0000 pinctrl-0 /pinctrl/uart7/uart7m2-xfer
	assert_ref_list "$dtb" /serial@fe6d0000 pinctrl-0 /pinctrl/uart9/uart9m2-xfer
}

assert_dfi_dtb() {
	local dtb=$1
	expect_eq "$(fdtget "$dtb" /dfi@fe230000 status)" disabled "unused DDR3 DFI monitor"
}

assert_preserved_board_dtb() {
	local dtb=$1 node
	for node in /mmc@fe310000 /mmc@fe2b0000 /serial@fe660000 /serial@fe690000 \
		/usb@fcc00000 /usb@fd000000 /usb@fd800000 /usb@fd840000 \
		/usb@fd880000 /usb@fd8c0000 /i2c@fe5d0000 /gpu@fde60000 \
		/vop@fe040000 /saradc@fe720000 /tsadc@fe710000; do
		expect_eq "$(fdtget "$dtb" "$node" status)" okay "preserve $node"
	done
	expect_eq "$(fdtget "$dtb" /aliases serial0)" /serial@fe660000 "console alias"
	expect_absent "$dtb" /aliases serial2
	expect_eq "$(fdtget "$dtb" /chosen stdout-path)" serial0:1500000n8 "UART console"
	# rk356x-u-boot.dtsi already overrides the board's 52 MHz ceiling to 200 MHz.
	# Keep the shipped merged DT unchanged; absence of HS200 is a separate check.
	expect_eq "$(fdtget "$dtb" /mmc@fe310000 max-frequency)" 200000000 "existing merged eMMC ceiling"
	expect_absent "$dtb" /mmc@fe310000 mmc-hs200-1_8v
	expect_eq "$(fdtget "$dtb" /mmc@fe2b0000 max-frequency)" 50000000 "SD speed retained"
	expect_eq "$(fdtget "$dtb" /mmc@fe2c0000 max-frequency)" 150000000 "SDIO speed retained"
	expect_eq "$(fdtget "$dtb" /sdio-pwrseq post-power-on-delay-ms)" 200 "Wi-Fi reset delay retained"
	assert_ref_list "$dtb" /mmc@fe2c0000 vmmc-supply /regulator-wlan-32k
	assert_ref_list "$dtb" /mmc@fe2c0000 mmc-pwrseq /sdio-pwrseq
	expect_eq "$(fdtget "$dtb" /regulator-wlan-32k clocks)" "$(fdtget "$dtb" /sdio-pwrseq clocks)" "Wi-Fi 32k reference retained"
	fdtget "$dtb" /regulator-wlan-32k regulator-always-on >/dev/null || fail "Wi-Fi 32k keepalive missing"
	expect_eq "$(fdtget "$dtb" /i2c@fdd40000/pmic@20 compatible)" rockchip,rk809 "PMIC retained"
	expect_eq "$(fdtget "$dtb" /i2c@fe5d0000/rtc@51 compatible)" haoyu,hym8563 "RTC retained"
	expect_eq "$(fdtget "$dtb" /aliases rtc0)" /i2c@fe5d0000/rtc@51 "battery-backed RTC number"
	expect_eq "$(fdtget "$dtb" /aliases rtc1)" /i2c@fdd40000/pmic@20 "PMIC RTC number via parent"
	# Do not seize the I2C3 pads with UART3. Preserve the shipped RNG status too:
	# upstream U-Boot overrides Linux's disabled RNG; this is not a new enablement.
	expect_eq "$(fdtget "$dtb" /serial@fe670000 status)" disabled "UART3 pin conflict guard"
	expect_eq "$(fdtget "$dtb" /rng@fe388000 status)" okay "existing merged RNG status"
}

assert_wireless_dtb "$BUILD_DIR/u-boot.dtb"
assert_lan_dtb "$BUILD_DIR/u-boot.dtb"
assert_peripherals_dtb "$BUILD_DIR/u-boot.dtb"
assert_dfi_dtb "$BUILD_DIR/u-boot.dtb"
assert_preserved_board_dtb "$BUILD_DIR/u-boot.dtb"
grep -Fqx "CONFIG_BOOTCOMMAND=\"$BOOTCOMMAND\"" "$BUILD_DIR/.config" ||
	fail "compiled boot order changed"
grep -Fqx 'CONFIG_ENV_IS_NOWHERE=y' "$BUILD_DIR/.config" ||
	fail "environment storage policy changed"
grep -Fqx 'CONFIG_OF_LIVE=y' "$BUILD_DIR/.config" ||
	fail "flat-DT handoff requires U-Boot's separate live device tree"
grep -aFq 'K11C: Maxio PHY %08x prepared' "$BUILD_DIR/u-boot" ||
	fail "K11C Maxio hook is absent from U-Boot"

# A missing status implicitly enables DFI: reject both omission and reversal.
for mutation in dfi-status-missing dfi-enabled; do
	cp "$BUILD_DIR/u-boot.dtb" "$BUILD_ROOT/mutant.dtb"
	case "$mutation" in
		dfi-status-missing) fdtput -d "$BUILD_ROOT/mutant.dtb" /dfi@fe230000 status ;;
		dfi-enabled) fdtput -t s "$BUILD_ROOT/mutant.dtb" /dfi@fe230000 status okay ;;
	esac
	if (assert_dfi_dtb "$BUILD_ROOT/mutant.dtb") >/dev/null 2>&1; then
		fail "DFI negative mutation was accepted: $mutation"
	fi
	echo "PASS: rejected DFI mutation: $mutation"
done

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

# Exercise omissions/inversions against the exact production DT, without rebuilding.
for mutation in hdmi-i2s analog-i2s codec-clock codec-dai analog-route amp-polarity header-i2c header-uart7 header-uart9 \
	analog-sysclk-missing analog-sysclk-wrong analog-fixed-missing analog-ratio analog-clock-override analog-sync; do
	cp "$BUILD_DIR/u-boot.dtb" "$BUILD_ROOT/mutant.dtb"
	case "$mutation" in
		hdmi-i2s) fdtput -t s "$BUILD_ROOT/mutant.dtb" /i2s@fe400000 status disabled ;;
		analog-i2s) fdtput -t s "$BUILD_ROOT/mutant.dtb" /i2s@fe410000 status disabled ;;
		codec-clock) fdtput -d "$BUILD_ROOT/mutant.dtb" /i2c@fdd40000/pmic@20 clocks ;;
		codec-dai) fdtput -t i "$BUILD_ROOT/mutant.dtb" /i2c@fdd40000/pmic@20 '#sound-dai-cells' 1 ;;
		analog-route) fdtput -d "$BUILD_ROOT/mutant.dtb" /rk809-sound simple-audio-card,routing ;;
		amp-polarity) fdtput -t x "$BUILD_ROOT/mutant.dtb" /audio-amplifier enable-gpios \
			"$(fdtget -t x "$BUILD_ROOT/mutant.dtb" /pinctrl/gpio@fe760000 phandle)" 11 1 ;;
		header-i2c) fdtput -t s "$BUILD_ROOT/mutant.dtb" /i2c@fe5c0000 status disabled ;;
		header-uart7) fdtput -t s "$BUILD_ROOT/mutant.dtb" /serial@fe6b0000 status disabled ;;
		header-uart9) fdtput -t s "$BUILD_ROOT/mutant.dtb" /serial@fe6d0000 status disabled ;;
		analog-sysclk-missing) fdtput -d "$BUILD_ROOT/mutant.dtb" /rk809-sound/simple-audio-card,codec system-clock-frequency ;;
		analog-sysclk-wrong) fdtput -t i "$BUILD_ROOT/mutant.dtb" /rk809-sound/simple-audio-card,codec system-clock-frequency 11289600 ;;
		analog-fixed-missing) fdtput -d "$BUILD_ROOT/mutant.dtb" /rk809-sound/simple-audio-card,codec system-clock-fixed ;;
		analog-ratio) fdtput -t i "$BUILD_ROOT/mutant.dtb" /rk809-sound simple-audio-card,mclk-fs 512 ;;
		analog-clock-override) fdtput -t x "$BUILD_ROOT/mutant.dtb" /rk809-sound/simple-audio-card,codec clocks \
			"$(fdtget -t x "$BUILD_ROOT/mutant.dtb" /clock-controller@fdd20000 phandle)" 48 ;;
		analog-sync) fdtput -d "$BUILD_ROOT/mutant.dtb" /i2s@fe410000 rockchip,trcm-sync-tx-only ;;
	esac
	if (assert_peripherals_dtb "$BUILD_ROOT/mutant.dtb") >/dev/null 2>&1; then
		fail "peripheral negative mutation was accepted: $mutation"
	fi
	echo "PASS: rejected peripheral mutation: $mutation"
done
for mutation in emmc-disabled emmc-speed console uart3-conflict rng rtc0-wrong rtc1-missing rtc1-wrong; do
	cp "$BUILD_DIR/u-boot.dtb" "$BUILD_ROOT/mutant.dtb"
	case "$mutation" in
		emmc-disabled) fdtput -t s "$BUILD_ROOT/mutant.dtb" /mmc@fe310000 status disabled ;;
		emmc-speed) fdtput -t i "$BUILD_ROOT/mutant.dtb" /mmc@fe310000 max-frequency 52000000 ;;
		console) fdtput -t s "$BUILD_ROOT/mutant.dtb" /aliases serial0 /serial@fe6b0000 ;;
		uart3-conflict) fdtput -t s "$BUILD_ROOT/mutant.dtb" /serial@fe670000 status okay ;;
		rng) fdtput -t s "$BUILD_ROOT/mutant.dtb" /rng@fe388000 status disabled ;;
		rtc0-wrong) fdtput -t s "$BUILD_ROOT/mutant.dtb" /aliases rtc0 /i2c@fdd40000/pmic@20 ;;
		rtc1-missing) fdtput -d "$BUILD_ROOT/mutant.dtb" /aliases rtc1 ;;
		rtc1-wrong) fdtput -t s "$BUILD_ROOT/mutant.dtb" /aliases rtc1 /i2c@fe5d0000/rtc@51 ;;
	esac
	if (assert_preserved_board_dtb "$BUILD_ROOT/mutant.dtb") >/dev/null 2>&1; then
		fail "board invariant negative mutation was accepted: $mutation"
	fi
done

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
	printf 'hdmi_audio_configured=true\n'
	printf 'rk809_audio_configured=true\n'
	printf 'rk809_fixed_hardware_rate_hz=48000\n'
	printf 'rk809_fixed_system_clock_hz=12288000\n'
	printf 'header_i2c3_uart7_uart9_configured=true\n'
	printf 'rtc0=hym8563\n'
	printf 'rtc1=rk809-parent-alias\n'
	printf 'linux_uboot_otp_duplicate_disabled=true\n'
	printf 'dfi_monitor_disabled=true\n'
	printf 'audio_hardware_playback_tested=false\n'
	printf 'negative_tests=pass\n'
} >"$OUTPUT.manifest.txt"

[[ -z "$(git -C "$UBOOT_TREE" status --porcelain --untracked-files=all)" ]] ||
	fail "U-Boot source checkout changed during the build"

echo "PASS: built $OUTPUT"
echo "SHA256: $output_sha256"
echo "Manifest: $OUTPUT.manifest.txt"
