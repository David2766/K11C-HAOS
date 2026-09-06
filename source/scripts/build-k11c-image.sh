#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
set -Eeuo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
PORT_ROOT=$(cd -- "${SCRIPT_DIR}/.." && pwd)

HAOS_IMAGE=""
HAOS_SHA256=""
UBOOT_TREE=""
DDR_BLOB=""
BL31_BLOB=""
OUTPUT_DIR=""
RELEASE_NAME=""

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

usage() {
	cat <<'EOF'
Usage: build-k11c-image.sh --haos-image IMAGE_OR_IMAGE_XZ \
  --haos-sha256 SHA256 --uboot-tree PATH --ddr-blob FILE --bl31 FILE \
  --output-dir PATH [--release-name NAME]

Builds the K11C U-Boot shim, preserves all eight generic AArch64 HAOS
partitions byte-for-byte, creates a ready-to-flash SD image, compresses it,
and writes release checksums and manifests. Existing outputs are not replaced.
EOF
}

while (($#)); do
	case "$1" in
		--haos-image) (($# >= 2)) || fail "$1 requires a file"; HAOS_IMAGE=$2; shift 2 ;;
		--haos-sha256) (($# >= 2)) || fail "$1 requires a hash"; HAOS_SHA256=${2,,}; shift 2 ;;
		--uboot-tree) (($# >= 2)) || fail "$1 requires a path"; UBOOT_TREE=$2; shift 2 ;;
		--ddr-blob) (($# >= 2)) || fail "$1 requires a file"; DDR_BLOB=$2; shift 2 ;;
		--bl31) (($# >= 2)) || fail "$1 requires a file"; BL31_BLOB=$2; shift 2 ;;
		--output-dir) (($# >= 2)) || fail "$1 requires a path"; OUTPUT_DIR=$2; shift 2 ;;
		--release-name) (($# >= 2)) || fail "$1 requires a name"; RELEASE_NAME=$2; shift 2 ;;
		-h|--help) usage; exit 0 ;;
		*) fail "unknown argument: $1" ;;
	esac
done

for command in awk basename grep install realpath sha256sum stat xz; do
	command -v "$command" >/dev/null 2>&1 || fail "missing command: $command"
done
[[ -s "$HAOS_IMAGE" ]] || fail "--haos-image is missing or empty"
[[ "$HAOS_SHA256" =~ ^[0-9a-f]{64}$ ]] || fail "--haos-sha256 is required"
[[ -n "$UBOOT_TREE" ]] || fail "--uboot-tree is required"
[[ -s "$DDR_BLOB" ]] || fail "--ddr-blob is missing or empty"
[[ -s "$BL31_BLOB" ]] || fail "--bl31 is missing or empty"
[[ -n "$OUTPUT_DIR" ]] || fail "--output-dir is required"

HAOS_IMAGE=$(realpath "$HAOS_IMAGE")
OUTPUT_DIR=$(realpath -m "$OUTPUT_DIR")
input_name=$(basename "$HAOS_IMAGE")
if [[ "$input_name" =~ ^haos_generic-aarch64-([0-9]+\.[0-9]+)\.img(\.xz)?$ ]]; then
	HAOS_VERSION=${BASH_REMATCH[1]}
else
	fail "HAOS input name does not identify a generic AArch64 release: $input_name"
fi

if [[ -z "$RELEASE_NAME" ]]; then
	RELEASE_NAME="k11c-haos-${HAOS_VERSION}-sd"
fi
[[ "$RELEASE_NAME" =~ ^[A-Za-z0-9._-]+$ ]] || fail "invalid release name"

mkdir -p "$OUTPUT_DIR"
UBOOT="$OUTPUT_DIR/u-boot-rockchip-k11c-${HAOS_VERSION}.bin"
IMAGE="$OUTPUT_DIR/${RELEASE_NAME}.img"
COMPRESSED="$IMAGE.xz"
RELEASE_MANIFEST="$OUTPUT_DIR/${RELEASE_NAME}.release.txt"
CHECKSUMS="$OUTPUT_DIR/SHA256SUMS"

for artifact in "$UBOOT" "$UBOOT.manifest.txt" "$UBOOT.control.dtb" \
	"$UBOOT.config" "$IMAGE" "$IMAGE.manifest.txt" "$COMPRESSED" \
	"$RELEASE_MANIFEST" "$CHECKSUMS"; do
	[[ ! -e "$artifact" ]] || fail "output already exists: $artifact"
done

"$SCRIPT_DIR/build-k11c-uboot.sh" \
	--uboot-tree "$UBOOT_TREE" \
	--ddr-blob "$DDR_BLOB" \
	--bl31 "$BL31_BLOB" \
	--output "$UBOOT"

"$SCRIPT_DIR/repack-generic-image.sh" \
	--input "$HAOS_IMAGE" \
	--input-sha256 "$HAOS_SHA256" \
	--uboot "$UBOOT" \
	--output "$IMAGE"

XZ_OPT=${XZ_OPT:--T0 -6} xz --keep "$IMAGE"
xz --test "$COMPRESSED"

image_sha256=$(sha256sum "$IMAGE" | awk '{print $1}')
compressed_sha256=$(sha256sum "$COMPRESSED" | awk '{print $1}')
streamed_sha256=$(xz -dc "$COMPRESSED" | sha256sum | awk '{print $1}')
[[ "$streamed_sha256" == "$image_sha256" ]] ||
	fail "compressed image does not reproduce the validated raw image"
image_bytes=$(stat -c %s "$IMAGE")
((image_bytes % 512 == 0)) || fail "release image is not sector aligned"
image_sectors=$((image_bytes / 512))
printf -v image_sectors_hex '%x' "$image_sectors"
printf -v image_bytes_hex '%x' "$image_bytes"

install -m 0644 "$PORT_ROOT/repository/THIRD_PARTY_NOTICES.md" \
	"$OUTPUT_DIR/THIRD_PARTY_NOTICES.md"
install -m 0644 "$PORT_ROOT/repository/licenses/rockchip-rkbin-LICENSE.txt" \
	"$OUTPUT_DIR/ROCKCHIP_RKBIN_LICENSE.txt"

{
	printf 'release.name=%s\n' "$RELEASE_NAME"
	printf 'board=KickPi K11C V1.2\n'
	printf 'haos.version=%s\n' "$HAOS_VERSION"
	printf 'haos.source_artifact=%s\n' "$input_name"
	printf 'haos.source_sha256=%s\n' "$HAOS_SHA256"
	printf 'haos.source_url=https://github.com/home-assistant/operating-system/releases/download/%s/%s\n' \
		"$HAOS_VERSION" "$input_name"
	printf 'image.artifact=%s\n' "$(basename "$COMPRESSED")"
	printf 'image.raw_sha256=%s\n' "$image_sha256"
	printf 'image.xz_sha256=%s\n' "$compressed_sha256"
	printf 'image.raw_bytes=%s\n' "$image_bytes"
	printf 'image.raw_bytes_hex=%s\n' "$image_bytes_hex"
	printf 'image.raw_sectors=%s\n' "$image_sectors"
	printf 'image.raw_sectors_hex=%s\n' "$image_sectors_hex"
	printf 'ready_to_flash=true\n'
	printf 'generic_haos_partitions_preserved=true\n'
	printf 'wifi_bluetooth_driver_included=false\n'
	printf 'seekwave_firmware_included=false\n'
	printf 'release_validation=pass\n'
} >"$RELEASE_MANIFEST"

(
	cd "$OUTPUT_DIR"
	sha256sum \
		"$(basename "$COMPRESSED")" \
		"$(basename "$RELEASE_MANIFEST")" \
		"$(basename "$IMAGE.manifest.txt")" \
		"$(basename "$UBOOT")" \
		"$(basename "$UBOOT.manifest.txt")" \
		"$(basename "$UBOOT.control.dtb")" \
		"$(basename "$UBOOT.config")" \
		THIRD_PARTY_NOTICES.md ROCKCHIP_RKBIN_LICENSE.txt >SHA256SUMS
	sha256sum --check --strict SHA256SUMS >/dev/null
)

echo "PASS: release artifacts created in $OUTPUT_DIR"
echo "Flash image: $COMPRESSED"
echo "SHA256: $compressed_sha256"
echo "Raw image retained for local validation: $IMAGE"
