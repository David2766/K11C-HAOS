#!/usr/bin/env bash

set -euo pipefail

ORIGINAL=""
CANDIDATE=""
UBOOT=""
REPORT=""
SHIFT_SECTORS=32768
SHIM_SECTOR=64
SECTOR_SIZE=512

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

usage() {
	cat <<'EOF'
Usage: validate-shim-image.sh --original IMAGE --candidate IMAGE \
  --uboot U_BOOT_ROCKCHIP_BIN [--report FILE]
EOF
}

while (($#)); do
	case "$1" in
		--original) ORIGINAL="$2"; shift 2 ;;
		--candidate) CANDIDATE="$2"; shift 2 ;;
		--uboot) UBOOT="$2"; shift 2 ;;
		--report) REPORT="$2"; shift 2 ;;
		--shift-sectors) SHIFT_SECTORS="$2"; shift 2 ;;
		--shim-sector) SHIM_SECTOR="$2"; shift 2 ;;
		-h|--help) usage; exit 0 ;;
		*) fail "unknown argument: $1" ;;
	esac
done

for command in cmp dd diff jq mtype sfdisk sgdisk sha256sum stat; do
	command -v "$command" >/dev/null || fail "missing command: $command"
done
for file in "$ORIGINAL" "$CANDIDATE" "$UBOOT"; do
	[[ -s "$file" ]] || fail "required file is missing or empty: $file"
done
[[ "$SHIFT_SECTORS" =~ ^[0-9]+$ ]] || fail "invalid shift sector count"
[[ "$SHIM_SECTOR" =~ ^[0-9]+$ ]] || fail "invalid shim sector"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
ORIGINAL_JSON="$TMP_DIR/original.json"
CANDIDATE_JSON="$TMP_DIR/candidate.json"
ORIGINAL_LAYOUT="$TMP_DIR/original-layout.json"
CANDIDATE_LAYOUT="$TMP_DIR/candidate-layout.json"
GRUB_CFG="$TMP_DIR/grub.cfg"

sfdisk --json "$ORIGINAL" > "$ORIGINAL_JSON"
sfdisk --json "$CANDIDATE" > "$CANDIDATE_JSON"

jq -e '.partitiontable.label == "gpt" and .partitiontable.sectorsize == 512' \
	"$ORIGINAL_JSON" >/dev/null || fail "original is not a 512-byte-sector GPT image"
jq -e '.partitiontable.label == "gpt" and .partitiontable.sectorsize == 512' \
	"$CANDIDATE_JSON" >/dev/null || fail "candidate is not a 512-byte-sector GPT image"

expected_labels='["hassos-boot","hassos-kernel0","hassos-system0","hassos-kernel1","hassos-system1","hassos-bootstate","hassos-overlay","hassos-data"]'
jq -e --argjson labels "$expected_labels" \
	'[.partitiontable.partitions[].name] == $labels' "$ORIGINAL_JSON" >/dev/null ||
	fail "original generic-aarch64 partition order changed"
jq -e --argjson labels "$expected_labels" \
	'[.partitiontable.partitions[].name] == $labels' "$CANDIDATE_JSON" >/dev/null ||
	fail "candidate generic-aarch64 partition order changed"

original_disk_guid="$(jq -r '.partitiontable.id | ascii_upcase' "$ORIGINAL_JSON")"
candidate_disk_guid="$(jq -r '.partitiontable.id | ascii_upcase' "$CANDIDATE_JSON")"
[[ "$original_disk_guid" == "$candidate_disk_guid" ]] ||
	fail "disk GUID changed"

original_size="$(stat -c %s "$ORIGINAL")"
candidate_size="$(stat -c %s "$CANDIDATE")"
(( original_size % SECTOR_SIZE == 0 )) || fail "original size is not sector aligned"
expected_size=$((original_size + SHIFT_SECTORS * SECTOR_SIZE))
(( candidate_size == expected_size )) ||
	fail "candidate size mismatch: expected $expected_size, got $candidate_size"

jq --argjson shift "$SHIFT_SECTORS" -S \
	'[.partitiontable.partitions[] | {
		start: (.start + $shift), size, type: (.type | ascii_upcase),
		uuid: (.uuid | ascii_upcase), name, attrs: (.attrs // null)
	}]' "$ORIGINAL_JSON" > "$ORIGINAL_LAYOUT"
jq -S \
	'[.partitiontable.partitions[] | {
		start, size, type: (.type | ascii_upcase),
		uuid: (.uuid | ascii_upcase), name, attrs: (.attrs // null)
	}]' "$CANDIDATE_JSON" > "$CANDIDATE_LAYOUT"
diff -u "$ORIGINAL_LAYOUT" "$CANDIDATE_LAYOUT" >/dev/null ||
	fail "partition identity, size, order, or shifted start differs"
sgdisk --verify "$CANDIDATE" >/dev/null 2>&1 || fail "candidate GPT verification failed"

first_partition_start="$(jq -r '.partitiontable.partitions[0].start' "$CANDIDATE_JSON")"
uboot_size="$(stat -c %s "$UBOOT")"
shim_offset=$((SHIM_SECTOR * SECTOR_SIZE))
shim_end=$((shim_offset + uboot_size))
first_partition_offset=$((first_partition_start * SECTOR_SIZE))
(( SHIM_SECTOR >= 34 )) || fail "shim overlaps the primary GPT"
(( shim_end < first_partition_offset )) || fail "shim overlaps the first partition"

uboot_hash="$(sha256sum "$UBOOT" | cut -d' ' -f1)"
candidate_uboot_hash="$(
	dd if="$CANDIDATE" iflag=skip_bytes,count_bytes skip="$shim_offset" \
		count="$uboot_size" bs=1M status=none | sha256sum | cut -d' ' -f1
)"
[[ "$uboot_hash" == "$candidate_uboot_hash" ]] || fail "raw U-Boot shim differs"

gap_before_size=$(((SHIM_SECTOR - 34) * SECTOR_SIZE))
cmp -s -n "$gap_before_size" -i "$((34 * SECTOR_SIZE)):0" \
	"$CANDIDATE" /dev/zero || fail "unexpected data between GPT and shim"
gap_after_size=$((first_partition_offset - shim_end))
cmp -s -n "$gap_after_size" -i "$shim_end:0" \
	"$CANDIDATE" /dev/zero || fail "unexpected data between shim and first partition"

boot_start="$(jq -r '.partitiontable.partitions[] | select(.name == "hassos-boot") | .start' "$CANDIDATE_JSON")"
boot_offset=$((boot_start * SECTOR_SIZE))
mtype -i "$CANDIDATE@@$boot_offset" ::/EFI/BOOT/BOOTAA64.EFI >/dev/null ||
	fail "EFI/BOOT/BOOTAA64.EFI is missing"
mtype -i "$CANDIDATE@@$boot_offset" ::/EFI/BOOT/grubenv >/dev/null ||
	fail "EFI/BOOT/grubenv is missing"
mtype -i "$CANDIDATE@@$boot_offset" ::/cmdline.txt >/dev/null ||
	fail "cmdline.txt is missing"
mtype -i "$CANDIDATE@@$boot_offset" ::/EFI/BOOT/grub.cfg > "$GRUB_CFG" ||
	fail "EFI/BOOT/grub.cfg is missing"

system0_uuid="$(jq -r '.partitiontable.partitions[] | select(.name == "hassos-system0") | .uuid | ascii_downcase' "$CANDIDATE_JSON")"
system1_uuid="$(jq -r '.partitiontable.partitions[] | select(.name == "hassos-system1") | .uuid | ascii_downcase' "$CANDIDATE_JSON")"
grep -Fq "root=PARTUUID=$system0_uuid" "$GRUB_CFG" ||
	fail "GRUB slot A PARTUUID does not match GPT"
grep -Fq "root=PARTUUID=$system1_uuid" "$GRUB_CFG" ||
	fail "GRUB slot B PARTUUID does not match GPT"

partition_report="$TMP_DIR/partitions.sha256"
: > "$partition_report"
partition_count="$(jq '.partitiontable.partitions | length' "$ORIGINAL_JSON")"
for ((index = 0; index < partition_count; index++)); do
	original_start="$(jq -r ".partitiontable.partitions[$index].start" "$ORIGINAL_JSON")"
	candidate_start="$(jq -r ".partitiontable.partitions[$index].start" "$CANDIDATE_JSON")"
	partition_size="$(jq -r ".partitiontable.partitions[$index].size" "$ORIGINAL_JSON")"
	partition_name="$(jq -r ".partitiontable.partitions[$index].name" "$ORIGINAL_JSON")"
	original_offset=$((original_start * SECTOR_SIZE))
	candidate_offset=$((candidate_start * SECTOR_SIZE))
	partition_bytes=$((partition_size * SECTOR_SIZE))
	original_hash="$(
		dd if="$ORIGINAL" iflag=skip_bytes,count_bytes bs=1M \
			skip="$original_offset" count="$partition_bytes" status=none |
			sha256sum | cut -d' ' -f1
	)"
	candidate_hash="$(
		dd if="$CANDIDATE" iflag=skip_bytes,count_bytes bs=1M \
			skip="$candidate_offset" count="$partition_bytes" status=none |
			sha256sum | cut -d' ' -f1
	)"
	[[ "$original_hash" == "$candidate_hash" ]] ||
		fail "partition content changed: $partition_name"
	printf 'partition.%s.sha256=%s\n' "$partition_name" "$candidate_hash" >> \
		"$partition_report"
done

if [[ -n "$REPORT" ]]; then
	mkdir -p "$(dirname "$REPORT")"
	{
		printf 'disk_guid=%s\n' "$candidate_disk_guid"
		printf 'shift_sectors=%s\n' "$SHIFT_SECTORS"
		printf 'shim_sector=%s\n' "$SHIM_SECTOR"
		printf 'shim_size=%s\n' "$uboot_size"
		printf 'shim_sha256=%s\n' "$uboot_hash"
		cat "$partition_report"
	} > "$REPORT"
fi

echo "PASS: K11C EFI shim image preserves the complete generic-aarch64 payload"
