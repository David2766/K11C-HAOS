#!/usr/bin/env bash

set -euo pipefail

HAOS_TREE="${HAOS_TREE:-/home/user/work/haos-18.2}"
RAUC_BUNDLE="${RAUC_BUNDLE:-$HAOS_TREE/output/images/haos_generic-aarch64-18.2.raucb}"
RAUC_TOOL="${RAUC_TOOL:-$HAOS_TREE/output/host/bin/rauc}"
META="$HAOS_TREE/buildroot-external/board/arm-uefi/generic-aarch64/meta"
HOOK="$HAOS_TREE/buildroot-external/ota/rauc-hook"
SYSTEM_CONF="$HAOS_TREE/output/target/etc/rauc/system.conf"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

for file in "$META" "$HOOK" "$SYSTEM_CONF" "$RAUC_BUNDLE" "$RAUC_TOOL"; do
	[[ -s "$file" ]] || fail "required file is missing or empty: $file"
done

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
BUNDLE_INFO="$TMP_DIR/bundle-info.txt"
"$RAUC_TOOL" info --no-verify "$RAUC_BUNDLE" > "$BUNDLE_INFO" 2>&1

assert_contract() {
	local meta="$1"
	local hook="$2"
	local system_conf="$3"
	local bundle_info="$4"

	grep -Fxq 'BOARD_ID=generic-aarch64' "$meta" || fail "wrong generic board ID"
	grep -Fxq 'BOOTLOADER=grub' "$meta" || fail "generic bootloader is not GRUB"
	grep -Fxq 'BOOT_SPL=false' "$meta" || fail "generic bundle unexpectedly includes SPL"
	grep -Fq "Compatible:     'haos-generic-aarch64'" "$bundle_info" ||
		fail "bundle compatibility changed"
	grep -Fq '3 Images:' "$bundle_info" || fail "bundle image count changed"
	for image_class in boot kernel rootfs; do
		grep -Fq "[$image_class]" "$bundle_info" ||
			fail "bundle is missing $image_class"
	done
	if grep -Fq '[spl]' "$bundle_info"; then
		fail "bundle contains SPL and would overwrite the raw shim"
	fi

	grep -Fq 'cp -f "${BOOT_MNT}"/*.txt "${BOOT_TMP}/" || true' "$hook" ||
		fail "RAUC hook no longer preserves root boot text configuration"
	grep -Fq 'cp -f "${BOOT_MNT}"/EFI/BOOT/grubenv "${BOOT_TMP}/" || true' "$hook" ||
		fail "RAUC hook no longer preserves GRUB slot state"
	grep -Fq 'cp -rf "${BOOT_NEW}"/* "${BOOT_MNT}/"' "$hook" ||
		fail "RAUC boot replacement behavior changed"

	grep -Fxq 'compatible=haos-generic-aarch64' "$system_conf" ||
		fail "installed RAUC compatibility changed"
	grep -Fxq 'bootloader=grub' "$system_conf" || fail "installed RAUC bootloader changed"
	for slot in hassos-boot hassos-kernel0 hassos-system0 hassos-kernel1 \
		hassos-system1; do
		grep -Fq "device=/dev/disk/by-partlabel/$slot" "$system_conf" ||
			fail "installed RAUC slot is missing: $slot"
	done
}

expect_rejected() {
	local label="$1"
	shift
	if (assert_contract "$@") >/dev/null 2>&1; then
		fail "negative test was not rejected: $label"
	fi
	echo "PASS: negative guard rejected $label"
}

assert_contract "$META" "$HOOK" "$SYSTEM_CONF" "$BUNDLE_INFO"

BAD_META="$TMP_DIR/meta-with-spl"
sed 's/^BOOT_SPL=false$/BOOT_SPL=true/' "$META" > "$BAD_META"
expect_rejected "generic BOOT_SPL enable" "$BAD_META" "$HOOK" "$SYSTEM_CONF" \
	"$BUNDLE_INFO"

BAD_HOOK="$TMP_DIR/hook-without-config-preserve"
sed '/BOOT_MNT.*\/\*\.txt.*BOOT_TMP/d' "$HOOK" > "$BAD_HOOK"
expect_rejected "removed boot text preservation" "$META" "$BAD_HOOK" \
	"$SYSTEM_CONF" "$BUNDLE_INFO"

BAD_INFO="$TMP_DIR/bundle-with-spl.txt"
cp "$BUNDLE_INFO" "$BAD_INFO"
printf '%s\n' '[spl]' >> "$BAD_INFO"
expect_rejected "SPL added to generic RAUC bundle" "$META" "$HOOK" \
	"$SYSTEM_CONF" "$BAD_INFO"

echo "PASS: generic-aarch64 RAUC updates leave the K11C raw shim outside the bundle"
