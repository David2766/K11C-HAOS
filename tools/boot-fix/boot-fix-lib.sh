#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Shared implementation. Device/paths are set by the host-only entry point.
set -eu
export LC_ALL=C

K11C_GUID=353A3772-554F-408B-A80B-05747613DEA7
K11C_SECTORS=61071360
K11C_FIRST=34816
K11C_TAIL=$((K11C_SECTORS - 33))

fail() { echo "FAIL: $*" >&2; exit 1; }
same() { cmp -s "$1" "$2" || fail "$3"; }

read_sectors() {
    dd if="$K11C_DEVICE" of="$4" bs=512 skip="$1" count="$2" 2>"$3"
    [ "$(wc -c < "$4")" -eq "$((512 * $2))" ] || fail 'Short disk read'
}

check_layout() {
    sfdisk --dump "$K11C_DEVICE" > "$1"
    grep -Fqx "label-id: $K11C_GUID" "$1" || fail 'Unexpected disk GUID; wrong disk or changed state'
    awk '
      /start=/ { n++; if (n == 1 && $0 !~ /start= *34816,/) exit 1 }
      END { if (n != 8) exit 1 }
    ' "$1" || fail 'Expected existing eight-partition K11C layout with first LBA 34816'
}

check_gpt_problem() {
    sgdisk --verify "$K11C_DEVICE" > "$1" 2>&1
    if grep -q '^No problems found\.' "$1"; then
        echo 'GPT_ALREADY_CLEAN'
        return
    fi
    grep -Fq "Problem: main header's disk GUID" "$1" &&
    grep -Fqx 'Identified 1 problems!' "$1" || fail 'Not the isolated disk-GUID mismatch; no disk write'
}

# Compare the actual on-disk GUIDs and entry arrays, not a tool's exit status.
# Header and entry CRC validation is supplied by the full sgdisk report below.
check_gpt_pair() {
    K11C_PAIR=$1
    read_sectors 0 34 "$K11C_BACKUP/read.log" "$K11C_PAIR.primary"
    read_sectors "$K11C_TAIL" 33 "$K11C_BACKUP/read.log" "$K11C_PAIR.backup"
    dd if="$K11C_PAIR.primary" of="$K11C_PAIR.guid-primary" bs=1 skip=568 count=16 2>"$K11C_BACKUP/read.log"
    dd if="$K11C_PAIR.backup" of="$K11C_PAIR.guid-backup" bs=1 skip=16440 count=16 2>"$K11C_BACKUP/read.log"
    same "$K11C_PAIR.guid-primary" "$K11C_PAIR.guid-backup" 'On-disk GPT GUIDs differ'
    dd if="$K11C_PAIR.primary" of="$K11C_PAIR.entries-primary" bs=512 skip=2 count=32 2>"$K11C_BACKUP/read.log"
    dd if="$K11C_PAIR.backup" of="$K11C_PAIR.entries-backup" bs=512 count=32 2>"$K11C_BACKUP/read.log"
    same "$K11C_PAIR.entries-primary" "$K11C_PAIR.entries-backup" 'On-disk GPT partition arrays differ'
}

verify_gpt_clean() {
    sgdisk --verify "$K11C_DEVICE" > "$K11C_BACKUP/verify.after" 2>&1
    cat "$K11C_BACKUP/verify.after"
    grep -q '^No problems found\.' "$K11C_BACKUP/verify.after" || fail 'GPT verification not clean'
    check_gpt_pair "$K11C_BACKUP/pair"
}

check_gpt_only() {
    check_backup
    verify_gpt_clean
    echo 'K11C_GPT_CHECK_PASS current_metadata=clean primary_and_partitions_unchanged=1 uboot_unchanged=1'
    if dmesg | grep -Fq "GPT:disk_guids don't match"; then
        echo 'NOTE: This boot retains an earlier GPT warning; check dmesg after the next normal reboot.'
    fi
}

backup_state() {
    [ ! -e "$K11C_BACKUP" ] || fail 'Backup directory already exists; do not overwrite it'
    mkdir -m 700 "$K11C_BACKUP"
    check_layout "$K11C_BACKUP/partitions.before"
    check_gpt_problem "$K11C_BACKUP/verify.before"
    read_sectors 0 34 "$K11C_BACKUP/read.log" "$K11C_BACKUP/gpt-primary.bin"
    read_sectors "$K11C_TAIL" 33 "$K11C_BACKUP/read.log" "$K11C_BACKUP/gpt-backup.bin"
    read_sectors 34 "$((K11C_FIRST - 34))" "$K11C_BACKUP/read.log" "$K11C_BACKUP/uboot-reserved.bin"
    cp "$K11C_CMDLINE" "$K11C_BACKUP/cmdline.before.txt"
    [ -s "$K11C_BACKUP/cmdline.before.txt" ] || fail 'Empty cmdline backup'
    (
        cd "$K11C_BACKUP"
        sha256sum partitions.before verify.before gpt-primary.bin gpt-backup.bin \
            uboot-reserved.bin cmdline.before.txt > SHA256SUMS
    )
    sync
    echo "K11C_BACKUP_READY=$K11C_BACKUP"
}

check_backup() {
    [ -d "$K11C_BACKUP" ] || fail 'Backup directory not found'
    (cd "$K11C_BACKUP" && sha256sum -c SHA256SUMS) || fail 'Backup checksum failure'
    check_layout "$K11C_BACKUP/partitions.current"
    same "$K11C_BACKUP/partitions.before" "$K11C_BACKUP/partitions.current" 'Partition layout changed since backup'
    read_sectors 0 34 "$K11C_BACKUP/read.log" "$K11C_BACKUP/primary.current"
    same "$K11C_BACKUP/gpt-primary.bin" "$K11C_BACKUP/primary.current" 'Primary GPT/MBR changed'
    read_sectors 34 "$((K11C_FIRST - 34))" "$K11C_BACKUP/read.log" "$K11C_BACKUP/reserved.current"
    same "$K11C_BACKUP/uboot-reserved.bin" "$K11C_BACKUP/reserved.current" 'U-Boot/reserved bytes changed'
}

repair_gpt() {
    check_backup
    check_gpt_problem "$K11C_BACKUP/verify.current"
    if grep -q '^No problems found\.' "$K11C_BACKUP/verify.current"; then
        verify_gpt_clean
        echo 'K11C_GPT_PASS already_clean=1 writes=0'
        return
    fi
    read_sectors "$K11C_TAIL" 33 "$K11C_BACKUP/read.log" "$K11C_BACKUP/secondary.current"
    same "$K11C_BACKUP/gpt-backup.bin" "$K11C_BACKUP/secondary.current" 'Backup GPT changed since backup'
    dd if="$K11C_BACKUP/gpt-primary.bin" of="$K11C_BACKUP/entries.primary" bs=512 skip=2 count=32 2>"$K11C_BACKUP/read.log"
    dd if="$K11C_BACKUP/secondary.current" of="$K11C_BACKUP/entries.secondary" bs=512 count=32 2>"$K11C_BACKUP/read.log"
    same "$K11C_BACKUP/entries.primary" "$K11C_BACKUP/entries.secondary" 'Partition arrays differ; not a GUID-only repair'
    echo 'Updating only GPT metadata using the existing primary disk GUID.'
    sgdisk --disk-guid="$K11C_GUID" "$K11C_DEVICE" > "$K11C_BACKUP/repair.log" 2>&1 || {
        cat "$K11C_BACKUP/repair.log"
        fail 'GPT utility failed; keep backups and send the log; no automatic rollback'
    }
    sync
    check_backup
    read_sectors "$K11C_TAIL" 32 "$K11C_BACKUP/read.log" "$K11C_BACKUP/entries.after"
    dd if="$K11C_BACKUP/gpt-backup.bin" of="$K11C_BACKUP/entries.before" bs=512 count=32 2>"$K11C_BACKUP/read.log"
    same "$K11C_BACKUP/entries.before" "$K11C_BACKUP/entries.after" 'Backup partition entries changed'
    # The secondary header may change only its disk GUID and header CRC.
    for K11C_SLICE in '0 16' '20 36' '72 440'; do
        set -- $K11C_SLICE
        dd if="$K11C_BACKUP/gpt-backup.bin" of="$K11C_BACKUP/header.before" bs=1 skip="$((16384 + $1))" count="$2" 2>"$K11C_BACKUP/read.log"
        dd if="$K11C_DEVICE" of="$K11C_BACKUP/header.after" bs=1 skip="$(((K11C_SECTORS - 1) * 512 + $1))" count="$2" 2>"$K11C_BACKUP/read.log"
        same "$K11C_BACKUP/header.before" "$K11C_BACKUP/header.after" 'Unexpected secondary GPT header change'
    done
    verify_gpt_clean
    echo 'K11C_GPT_PASS primary_and_partitions_unchanged=1 uboot_unchanged=1'
}

make_console_candidate() {
    awk '
      { for (i=1; i<=NF; i++) {
          if ($i == "console=ttyS0") plain++;
          if ($i ~ /^console=ttyS0/) total++;
        }
      }
      END { if (plain != 1 || total != 1) exit 1 }
    ' "$K11C_BACKUP/cmdline.before.txt" || fail 'Expected one console=ttyS0 token in original cmdline'
    sed -E 's/(^|[[:space:]])console=ttyS0([[:space:]]|$)/\1console=ttyS0,1500000n8\2/' \
        "$K11C_BACKUP/cmdline.before.txt" > "$K11C_BACKUP/cmdline.expected.txt"
    grep -Eq '(^|[[:space:]])console=ttyS0,1500000n8([[:space:]]|$)' \
        "$K11C_BACKUP/cmdline.expected.txt" || fail 'Console replacement failed'
}

install_cmdline() {
    K11C_TEMP=$(mktemp "${K11C_CMDLINE}.k11c.XXXXXX")
    cp "$1" "$K11C_TEMP"
    same "$1" "$K11C_TEMP" 'Temporary cmdline copy mismatch'
    sync
    mv -f "$K11C_TEMP" "$K11C_CMDLINE"
    sync
    same "$1" "$K11C_CMDLINE" 'Installed cmdline mismatch'
}

set_console() {
    check_backup
    make_console_candidate
    if cmp -s "$K11C_CMDLINE" "$K11C_BACKUP/cmdline.expected.txt"; then
        echo 'K11C_CONSOLE_STAGED already_set=1 reboot_required=1'
        return
    fi
    same "$K11C_CMDLINE" "$K11C_BACKUP/cmdline.before.txt" 'cmdline changed since backup; refusing overwrite'
    install_cmdline "$K11C_BACKUP/cmdline.expected.txt"
    cat "$K11C_CMDLINE"
    echo 'K11C_CONSOLE_STAGED baud=1500000 reboot_required=1 live_tty_unchanged=1'
}

restore_console() {
    check_backup
    make_console_candidate
    if cmp -s "$K11C_CMDLINE" "$K11C_BACKUP/cmdline.before.txt"; then
        echo 'K11C_CONSOLE_RESTORED already_original=1'
        return
    fi
    same "$K11C_CMDLINE" "$K11C_BACKUP/cmdline.expected.txt" 'cmdline has later edits; refusing overwrite'
    install_cmdline "$K11C_BACKUP/cmdline.before.txt"
    echo 'K11C_CONSOLE_RESTORED reboot_required=1'
}
