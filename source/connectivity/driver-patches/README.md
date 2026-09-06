# SeekWave driver patches

These patches are intended for the K11C vendor copy of the SeekWave
VS/SWT6621S driver. They do not contain the complete corresponding driver
source.

The patches modify files carrying different notices. Most Wi-Fi and platform
driver files are marked `GPL-2.0-only`; the Bluetooth driver includes a GNU
GPL version 2-or-later notice; a small number of supporting headers in the
vendor tree are marked MIT. Each patch follows the license of the source file
it modifies and does not place that source under the repository's AGPL
license.

The newly added `include/linux/platform_data/skw_firmware.h` in patch 0006 is
explicitly marked `GPL-2.0-only`.

Firmware and NVRAM binary files are not source code and are not licensed by
these patches. The public export omits them because no redistribution grant
was identified in the obtained vendor package.

See the repository's root `THIRD_PARTY_NOTICES.md` for the complete boundary.
