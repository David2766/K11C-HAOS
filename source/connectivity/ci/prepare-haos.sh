#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Prepare official Generic AArch64 kernel/toolchain ONLY, never an OS image.
set -Eeuo pipefail
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
[[ $# == 2 && "$1" =~ ^[0-9]+\.[0-9]+$ ]] || { echo 'Usage: prepare-haos.sh RELEASE NEW_DIRECTORY' >&2; exit 1; }
release=$1
dest=$(realpath -m "$2")
if [[ -e "$dest" ]]; then
    [[ -d "$dest/.git" ]] || { echo 'Existing path is not an HAOS checkout' >&2; exit 1; }
    [[ $(git -C "$dest" remote get-url origin) == https://github.com/home-assistant/operating-system.git ]]
    [[ $(git -C "$dest" describe --tags --exact-match HEAD) == "$release" ]]
else
    git clone --branch "$release" --depth 1 --recurse-submodules --shallow-submodules \
        https://github.com/home-assistant/operating-system.git "$dest"
fi
# Official defaults use /cache, which is not writable on a normal runner/WSL.
# Override build storage only; do not change the target kernel configuration.
build_args=(BR2_DL_DIR="$dest/downloads" BR2_CCACHE_DIR="$dest/ccache" BR2_JLEVEL="${JOBS:-8}")
make -C "$dest" "${build_args[@]}" generic_aarch64-config
# Buildroot linux target builds the exact configured kernel and external module
# metadata. No rootfs, disk image, U-Boot, flashing or YAML ROM compilation.
make -C "$dest" "${build_args[@]}" linux
git -C "$dest" rev-parse HEAD
