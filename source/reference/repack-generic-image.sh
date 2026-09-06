#!/usr/bin/env bash

set -euo pipefail

INPUT=""
INPUT_SHA256=""
UBOOT="/home/user/work/k11c-port-build/board-definitions/u-boot/u-boot-rockchip.bin"
OUTPUT=""
SHIFT_SECTORS=32768
SHIM_SECTOR=64
SECTOR_SIZE=512
COPY_BLOCK_SIZE=1048576
RUN_NEGATIVE_TESTS="${RUN_NEGATIVE_TESTS:-1}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR="$SCRIPT_DIR/validate-shim-image.sh"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

usage() {
	cat <<'EOF'
Usage: repack-generic-image.sh --input IMAGE_OR_IMAGE_XZ \
  --input-sha256 SHA256 --output K11C_IMAGE [--uboot U_BOOT_ROCKCHIP_BIN]

The output must not already exist. Set RUN_NEGATIVE_TESTS=0 only for a quick
local rerun; release validation must keep the default.
EOF
}

while (($#)); do
	case "$1" in
		--input) INPUT="$2"; shift 2 ;;
		--input-sha256) INPUT_SHA256="${2,,}"; shift 2 ;;
		--uboot) UBOOT="$2"; shift 2 ;;
		--output) OUTPUT="$2"; shift 2 ;;
		-h|--help) usage; exit 0 ;;
		*) fail "unknown argument: $1" ;;
	esac
done

for command in awk cp dd jq mktemp sfdisk sgdisk sha256sum stat truncate xz; do
	command -v "$command" >/dev/null || fail "missing command: $command"
done
[[ -s "$INPUT" ]] || fail "input image is missing or empty: $INPUT"
[[ -s "$UBOOT" ]] || fail "U-Boot image is missing or empty: $UBOOT"
[[ -x "$VALIDATOR" ]] || fail "validator is missing or not executable: $VALIDATOR"
[[ "$INPUT_SHA256" =~ ^[0-9a-f]{64}$ ]] || fail "--input-sha256 is required"
[[ -n "$OUTPUT" ]] || fail "--output is required"
[[ "$RUN_NEGATIVE_TESTS" == "0" || "$RUN_NEGATIVE_TESTS" == "1" ]] ||
	fail "RUN_NEGATIVE_TESTS must be 0 or 1"
[[ ! -e "$OUTPUT" ]] || fail "output already exists: $OUTPUT"
[[ ! -e "$OUTPUT.manifest.txt" ]] || fail "manifest already exists: $OUTPUT.manifest.txt"

actual_input_hash="$(sha256sum "$INPUT" | cut -d' ' -f1)"
[[ "$actual_input_hash" == "$INPUT_SHA256" ]] ||
	fail "input SHA-256 mismatch: expected $INPUT_SHA256, got $actual_input_hash"

output_dir="$(dirname "$OUTPUT")"
mkdir -p "$output_dir"
TMP_DIR="$(mktemp -d "$output_dir/.k11c-repack.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT
SOURCE_IMAGE="$INPUT"
if [[ "$INPUT" == *.xz ]]; then
	SOURCE_IMAGE="$TMP_DIR/source.img"
	xz -dc "$INPUT" > "$SOURCE_IMAGE"
fi

(( $(stat -c %s "$SOURCE_IMAGE") % SECTOR_SIZE == 0 )) ||
	fail "decompressed input size is not sector aligned"
SOURCE_DUMP="$TMP_DIR/source.sfdisk"
SHIFTED_DUMP="$TMP_DIR/shifted.sfdisk"
CANDIDATE="$TMP_DIR/candidate.img"
REPORT="$TMP_DIR/manifest.txt"
sfdisk --dump "$SOURCE_IMAGE" > "$SOURCE_DUMP"
grep -Fxq 'label: gpt' "$SOURCE_DUMP" || fail "input image is not GPT"
grep -Fxq 'sector-size: 512' "$SOURCE_DUMP" || fail "input sector size is not 512"

awk -v shift="$SHIFT_SECTORS" '
	/^device:/ { next }
	/^last-lba:/ { print "last-lba: " ($2 + shift); next }
	index($0, "start=") {
		position = index($0, "start=")
		rest = substr($0, position + 6)
		comma = index(rest, ",")
		old_start = substr(rest, 1, comma - 1) + 0
		printf "%sstart=%12d%s\n", substr($0, 1, position - 1), \
			old_start + shift, substr(rest, comma)
		next
	}
	{ print }
' "$SOURCE_DUMP" > "$SHIFTED_DUMP"

source_size="$(stat -c %s "$SOURCE_IMAGE")"
candidate_size=$((source_size + SHIFT_SECTORS * SECTOR_SIZE))
shift_bytes=$((SHIFT_SECTORS * SECTOR_SIZE))
(( shift_bytes % COPY_BLOCK_SIZE == 0 )) ||
	fail "shim reservation is not aligned to the bulk copy size"
truncate -s "$candidate_size" "$CANDIDATE"
dd if="$SOURCE_IMAGE" of="$CANDIDATE" bs="$COPY_BLOCK_SIZE" \
	seek="$((shift_bytes / COPY_BLOCK_SIZE))" conv=sparse,notrunc status=none
# Remove the copied primary GPT before installing the shifted table.
dd if=/dev/zero of="$CANDIDATE" bs="$SECTOR_SIZE" seek="$SHIFT_SECTORS" \
	count=34 conv=notrunc status=none
sfdisk --force "$CANDIDATE" < "$SHIFTED_DUMP" >/dev/null
sgdisk --verify "$CANDIDATE" >/dev/null 2>&1 || fail "rewritten GPT is invalid"

dd if="$UBOOT" of="$CANDIDATE" bs="$SECTOR_SIZE" seek="$SHIM_SECTOR" \
	conv=notrunc status=none
"$VALIDATOR" --original "$SOURCE_IMAGE" --candidate "$CANDIDATE" \
	--uboot "$UBOOT" --report "$REPORT"

expect_rejected() {
	local label="$1"
	local image="$2"
	if "$VALIDATOR" --original "$SOURCE_IMAGE" --candidate "$image" \
		--uboot "$UBOOT" >/dev/null 2>&1; then
		fail "negative test was not rejected: $label"
	fi
	echo "PASS: negative guard rejected $label"
}

if [[ "$RUN_NEGATIVE_TESTS" == "1" ]]; then
	SHIM_BROKEN="$TMP_DIR/shim-broken.img"
	cp --reflink=auto --sparse=always "$CANDIDATE" "$SHIM_BROKEN"
	dd if=/dev/zero of="$SHIM_BROKEN" bs="$SECTOR_SIZE" seek="$SHIM_SECTOR" \
		count=1 conv=notrunc status=none
	expect_rejected "corrupted raw U-Boot shim" "$SHIM_BROKEN"

	GPT_BROKEN="$TMP_DIR/gpt-broken.img"
	cp --reflink=auto --sparse=always "$CANDIDATE" "$GPT_BROKEN"
	sgdisk --partition-guid=3:11111111-2222-3333-4444-555555555555 \
		"$GPT_BROKEN" >/dev/null
	expect_rejected "changed rootfs partition GUID" "$GPT_BROKEN"

	BOOT_BROKEN="$TMP_DIR/boot-broken.img"
	cp --reflink=auto --sparse=always "$CANDIDATE" "$BOOT_BROKEN"
	boot_start="$(sfdisk --json "$BOOT_BROKEN" | jq -r \
		'.partitiontable.partitions[] | select(.name == "hassos-boot") | .start')"
	dd if=/dev/zero of="$BOOT_BROKEN" bs=1 seek="$((boot_start * SECTOR_SIZE + 510))" \
		count=1 conv=notrunc status=none
	expect_rejected "changed generic boot partition" "$BOOT_BROKEN"
fi

output_hash="$(sha256sum "$CANDIDATE" | cut -d' ' -f1)"
{
	printf 'source_artifact=%s\n' "$(basename "$INPUT")"
	printf 'source_artifact_sha256=%s\n' "$actual_input_hash"
	printf 'output_sha256=%s\n' "$output_hash"
	cat "$REPORT"
} > "$TMP_DIR/final-manifest.txt"
mv "$CANDIDATE" "$OUTPUT"
mv "$TMP_DIR/final-manifest.txt" "$OUTPUT.manifest.txt"

echo "PASS: created $OUTPUT"
echo "SHA256: $output_hash"
echo "Manifest: $OUTPUT.manifest.txt"
