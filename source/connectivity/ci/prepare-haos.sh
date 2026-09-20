#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Prepare official Generic AArch64 kernel/toolchain ONLY, never an OS image.
set -Eeuo pipefail
[[ $# == 2 && "$1" =~ ^[0-9]+\.[0-9]+$ ]] || { echo 'Usage: prepare-haos.sh RELEASE NEW_DIRECTORY' >&2; exit 1; }
release=$1
dest=$(realpath -m "$2")
test ! -e "$dest"
git clone --branch "$release" --depth 1 --recurse-submodules --shallow-submodules \
    https://github.com/home-assistant/operating-system.git "$dest"
make -C "$dest" generic_aarch64-config
# Buildroot linux target builds the exact configured kernel and external module
# metadata. No rootfs, disk image, U-Boot, flashing or YAML ROM compilation.
make -C "$dest" linux
git -C "$dest" rev-parse HEAD
