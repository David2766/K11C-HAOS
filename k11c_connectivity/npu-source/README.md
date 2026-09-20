# RKNPU 900 MHz default and r22 read-only OTP module sources

Upstream: https://github.com/w568w/rknpu-module

Commit: `a8792fe6b633f90cf2c6808267cac327537ab4cf`

This directory contains the modified C/header files and Kbuild used for
the K11C modules, including the 0.5.1 default-request change. Retain their upstream GPL-2.0 notices. `COPYING`
contains the GPL-2.0 license. This driver is not licensed under the App's AGPL.

Changes: Linux 6.18 devfreq header compatibility, plus initial OPP application,
voltage/clock verification, managed OPP cleanup and probe failure propagation.
The earlier r19b changes are recorded in `rknpu-initial-opp.patch`; that historical
patch is not the complete r22 change set. All changes are already applied to
these sources. DRM GEM/CMA is used, not DMA heaps. `VENDOR-POLICY.md` describes
the manufacturer selection/thermal/runtime port and its source references.

Build only the external module against a prepared, matching HAOS kernel tree:

```sh
make -C "$HAOS_KERNEL_BUILD" M="$PWD" ARCH=arm64 \
  CROSS_COMPILE="$HAOS_CROSS_COMPILE" -j4 modules
```

`HAOS_KERNEL_BUILD` must point to the prepared `linux-6.18.39` build with its
original `.config`, generated headers and `Module.symvers`.
`HAOS_CROSS_COMPILE` is the full path/prefix ending in
`aarch64-buildroot-linux-gnu-` from that same HAOS toolchain.
The kernel in this bundle is `6.18.39-haos`, GCC 13.4.0.

Do not use `modules_install`, replace the host kernel, or apply a generic DT
overlay from another board. The K11C r22 DT is supplied by U-Boot.

Packaged rknpu.ko SHA256:
`dea3d21a0a794dbd752da0c8f2b2de2a873d9aed53aeaf6fb0158d6cda667326`

srcversion: `CD8FDCD74670016E17E2BFD`.

Packaged k11c_rk3568_otp.ko SHA256:
`992cd452773341b2df9d2cb47e59744cc04ad6e982f586b767474038fc68ae15`

srcversion: `17ADBE45E3464BE801709BF`; version: `r22.1`.

The loader pins these exact binaries. The previous r22 module passed the
600/900 MHz board comparison; startup with this rebuilt default-profile module
has not yet been checked on the board.
A separately rebuilt or newer-kernel
module must be validated and the loader's compatibility/hash pins updated;
changing only the checksum does not establish hardware compatibility.
