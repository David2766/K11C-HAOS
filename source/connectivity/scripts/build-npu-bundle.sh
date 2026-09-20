#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Build BOTH external modules against a configured, fully built HAOS kernel.
set -Eeuo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
[[ $# == 3 ]] || { echo 'Usage: build-npu-bundle.sh HAOS_TREE APP_DIR WORK_PARENT' >&2; exit 1; }
haos=$(realpath "$1")
app=$(realpath "$2")
work=$(realpath "$3")
mapfile -t kernels < <(find "$haos/output/build" -maxdepth 1 -type d -name 'linux-[0-9]*' -exec test -f '{}/.config' \; -print)
[[ ${#kernels[@]} == 1 ]] || { echo 'Expected exactly one configured kernel' >&2; exit 1; }
kernel=${kernels[0]}
cross="$haos/output/host/bin/aarch64-buildroot-linux-gnu-"
test -x "${cross}gcc"
test -s "$kernel/Module.symvers"
release=$(make -s -C "$kernel" ARCH=arm64 kernelrelease)
stage=$(mktemp -d "$work/npu-build.XXXXXX")
# Retained as evidence, never writes the source App or a running board.
cp -a "$app/npu-source/src" "$stage/"
cp "$app/npu-source/Kbuild" "$stage/"
make -C "$kernel" M="$stage" ARCH=arm64 CROSS_COMPILE="$cross" -j"${JOBS:-4}" modules
out="$app/modules/$release/npu"
install -d "$out"
for name in rknpu k11c_rk3568_otp; do
    install -m 0644 "$stage/$name.ko" "$out/"
    "${cross}nm" -u "$out/$name.ko" | awk '$1 == "U" {print $2}' | LC_ALL=C sort -u > "$stage/$name.imports"
    awk '{print $2}' "$kernel/Module.symvers" | LC_ALL=C sort -u > "$stage/kernel.exports"
    LC_ALL=C comm -23 "$stage/$name.imports" "$stage/kernel.exports" > "$stage/$name.missing"
    test ! -s "$stage/$name.missing"
done
python3 "$here/kernel-bundle.py" write "$out"
echo "NPU_EXTERNAL_BUILD_PASS kernel=$release hardware=NOT_TESTED"
