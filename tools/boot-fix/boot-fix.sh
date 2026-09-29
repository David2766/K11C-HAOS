#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
set -eu
K11C_HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$K11C_HERE/boot-fix-lib.sh"

case "${1:-}" in
    backup|gpt|check-gpt|console|restore-console|check) ;;
    *) fail 'Usage: sh boot-fix.sh backup|gpt|check-gpt|console|restore-console|check /mnt/data/supervisor/share/k11c-boot-backup-NAME' ;;
esac
[ "$#" -eq 2 ] || fail 'Provide the backup directory as the second argument'
[ "$(id -u)" = 0 ] || fail 'Run on HAOS host serial # as root'
[ "$(cat /proc/1/comm)" = systemd ] || fail 'Use HAOS host #, not the SSH App'
grep -q '^ID=haos$' /etc/os-release || fail 'Not HAOS'
tr '\000' '\n' < /sys/firmware/devicetree/base/compatible | grep -Fxq kickpi,k11c || fail 'Not K11C'
K11C_DEVICE=/dev/mmcblk0
K11C_CMDLINE=/mnt/boot/cmdline.txt
K11C_BACKUP=$2
case "$K11C_BACKUP" in
    /mnt/data/supervisor/share/k11c-boot-backup-*) ;;
    *) fail 'Use the documented share backup directory prefix' ;;
esac
case "${K11C_BACKUP#/mnt/data/supervisor/share/k11c-boot-backup-}" in
    ''|*[!a-zA-Z0-9_-]*) fail 'Invalid backup directory suffix' ;;
esac
[ -b "$K11C_DEVICE" ] || fail 'eMMC block device missing'
[ "$(readlink -f /sys/class/mmc_host/mmc0)" = /sys/devices/platform/fe310000.mmc/mmc_host/mmc0 ] || fail 'mmc0 is not eMMC controller'
[ "$(cat /sys/class/block/mmcblk0/size)" -eq "$K11C_SECTORS" ] || fail 'Unexpected eMMC size'
[ "$(cat /sys/class/block/mmcblk0/queue/logical_block_size)" -eq 512 ] || fail 'Unexpected sector size'
[ -r "$K11C_CMDLINE" ] || fail 'Boot cmdline missing; do not create a replacement'
for K11C_TOOL in sgdisk sfdisk dd cmp sha256sum awk sed mktemp sync findmnt; do
    command -v "$K11C_TOOL" >/dev/null || fail "Missing command: $K11C_TOOL"
done
K11C_BOOT_SOURCE=$(findmnt -rn -o SOURCE --target /mnt/boot)
[ "$(readlink -f "$K11C_BOOT_SOURCE")" = /dev/mmcblk0p1 ] || fail '/mnt/boot is not the eMMC boot partition'

case "$1" in
    backup)
        backup_state
        dmesg > "$K11C_BACKUP/kernel.before.txt"
        cat /proc/cmdline > "$K11C_BACKUP/running-cmdline.before.txt"
        ;;
    gpt) repair_gpt ;;
    check-gpt) check_gpt_only ;;
    console) set_console ;;
    restore-console) restore_console ;;
    check)
        check_backup
        sgdisk --verify "$K11C_DEVICE" > "$K11C_BACKUP/verify.check" 2>&1
        cat "$K11C_BACKUP/verify.check"
        grep -q '^No problems found\.' "$K11C_BACKUP/verify.check" || fail 'GPT is not clean'
        grep -Eq '(^| )console=ttyS0,1500000n8( |$)' /proc/cmdline || fail 'New console argument not active; reboot needed?'
        K11C_BAUD=$(stty -F /dev/ttyS0 speed)
        [ "$K11C_BAUD" = 1500000 ] || fail "Current TTY speed is $K11C_BAUD, not 1500000"
        if dmesg | grep -Fq "GPT:disk_guids don't match"; then
            fail 'This boot still logged GPT mismatch; confirm a reboot after repair'
        fi
        printf 'CONSOLE_BAUD=%s\n' "$K11C_BAUD"
        dmesg | grep -Ei 'console.*enabled|Run /sbin/init|systemd.*running in system mode|hym8563|WIFIREADY|BTREADY' | tail -n 20
        echo 'K11C_BOOT_FIX_PASS GPT=clean console=1500000'
        ;;
esac
