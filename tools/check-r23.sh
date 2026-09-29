#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Run on the HAOS host. Read-only: no clock writes, module changes or restarts.
set -u
K11C_DT=/sys/firmware/devicetree/base
K11C_FAILED=0
fail() { printf 'FAIL: %s\n' "$*"; K11C_FAILED=1; }
dt_string() { tr -d '\000' < "$K11C_DT/$1"; }
expect() {
    if [ "$1" = "$2" ]; then
        printf 'PASS: %s=%s\n' "$3" "$1"
    else
        fail "$3 expected=$2 actual=$1"
    fi
}

printf 'K11C_R23_CHECK kernel=%s\n' "$(uname -r)"
expect "$(dt_string aliases/rtc0)" /i2c@fe5d0000/rtc@51 rtc0_alias
expect "$(dt_string aliases/rtc1)" /i2c@fdd40000/pmic@20 rtc1_alias
expect "$(dt_string nvmem@fe38c000/status)" disabled duplicate_otp_status

printf '\nRTC devices (date/time below are UTC):\n'
for K11C_RTC in /sys/class/rtc/rtc0 /sys/class/rtc/rtc1; do
    printf '%s\n' "$K11C_RTC"
    cat "$K11C_RTC/name" "$K11C_RTC/date" "$K11C_RTC/time" || fail "Cannot read $K11C_RTC"
done
case "$(cat /sys/class/rtc/rtc0/name 2>/dev/null)" in
    rtc-hym8563*) printf 'PASS: rtc0 is HYM8563\n' ;;
    *) fail 'rtc0 is not HYM8563' ;;
esac
case "$(cat /sys/class/rtc/rtc1/name 2>/dev/null)" in
    rk808-rtc*) printf 'PASS: rtc1 is the RK809 RTC\n' ;;
    *) fail 'rtc1 is not the RK809 RTC' ;;
esac
expect "$(cat /sys/class/rtc/rtc0/hctosys 2>/dev/null)" 1 rtc0_boot_time_restored
date -u

printf '\nOTP/NPU binding and policy:\n'
K11C_OTP_DRIVER=$(readlink -f /sys/bus/platform/devices/fe38c000.otp/driver 2>/dev/null || true)
expect "$K11C_OTP_DRIVER" /sys/bus/platform/drivers/k11c-rk3568-otp otp_driver
if [ -e /sys/bus/platform/devices/fe38c000.nvmem ]; then
    fail 'Duplicate Linux OTP platform device still exists'
else
    printf 'PASS: duplicate Linux OTP platform device absent\n'
fi
K11C_POLICY=$(cat /sys/bus/platform/devices/fde40000.npu/vendor_policy 2>/dev/null || true)
printf '%s\n' "$K11C_POLICY"
case " $K11C_POLICY " in
    *' error=0 '*) printf 'PASS: NPU vendor policy error=0\n' ;;
    *) fail 'NPU vendor policy unavailable or not healthy; check Connectivity logs' ;;
esac

printf '\nRelevant boot log:\n'
dmesg | grep -Ei 'rtc|hym8563|fe38c000|K11C_VENDOR|K11C-NPU-OPP|WIFIREADY|BTREADY' | tail -n 60
if dmesg | grep -E 'fe38c000.*(EBUSY|error -16|failed)' >/dev/null; then
    fail 'OTP failure remains in this boot log'
fi
if [ "$K11C_FAILED" -eq 0 ]; then
    printf 'K11C_R23_CHECK_PASS: RTC boot-time restore and OTP/NPU checks passed\n'
    printf 'Audio/LAN/Wi-Fi/BT operation and battery retention are separate hardware checks.\n'
fi
exit "$K11C_FAILED"
