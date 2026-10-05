# Recovery RAM Loader read access

## Input and scope

The installer keeps the original `k11c-usb-loader-v1.23.114.bin` on disk.
Its SHA-256 is `b0228bbe9d3be4ea13f399df58289a82b5dfe62d1f67d75512a9ddd2d9b25454`.
The manufacturer's boot_merger package combines RK3566 DDR v1.23, RK356x
USB plug v1.17 and SPL v1.14. The installer sends CODE471/CODE472 to RAM;
it never sends FlashBoot through this preparation path.

Only the CODE472 upload buffer is modified. Persistent r24 U-Boot, DDR,
HAOS partitions, GPT and the signed Windows USB driver are unchanged.
No automatic reset, storage switch or re-upload to an already-running
restricted Loader is used. Re-enter physical MASKROM to replace that Loader.

## Verified reason

Rockchip intentionally restricted reads beyond 32 MiB to protect user data
and firmware from extraction:
https://github.com/rockchip-linux/u-boot/commit/628c8271948b899f8b016e16d4c310ee40547c5d

The RAM USB plug is a separate vendor binary, not that U-Boot source build.
Its own disassembly and physical reads were checked. The original reports
capabilities `3707000000000000` (Read LBA On clear), initializes a 65536-sector
limit and returns 0xCC with successful transfer status beyond that start LBA.
LBA 65535 count 2 and LBA 65536 count 1 returned different bytes for the same
sector. The physical disk's GPT passed Linux sgdisk verification.

## Exact substitutions

Offsets are relative to the 100352-byte padded CODE472 payload:

| Offset | Original AArch64 instruction | RAM instruction | Purpose |
| --- | --- | --- | --- |
| 0x975c | `mov w8, #0x10000` (0x52a00028) | `mov w8, #-1` (0x12800008) | Initialize read limit to UINT32_MAX |
| 0x4588 | `mov w8, #0x437` (0x528086e8) | `mov w8, #0x43f` (0x528087e8) | Advertise Read LBA On truthfully |

Original padded payload SHA-256:
`10865671ac54b81bd6e59358a8635b94542ce2b65939674d0292c5a1dfac08d9`

Patched RAM payload SHA-256:
`c70336dc01d4243ed10935b6b029fc46e9f129f9b6c0c5f6e7c4838428853ee7`

These change four bytes inside two instructions. All other payload bytes
remain identical. Original initialization stores the limit at 0x459684 in
the binary-relative execution mapping. Both read pipelines load that limit
and select either the storage reader (0x9888) or 0xCC fill. The eMMC capability
handler does not reset the limit; supported alternate-storage branches already
set UINT32_MAX. The installer accepts only selected eMMC.

The 32-bit protocol's last usable address is still bounded by the reported
sector count and each derived write plan. This change does not widen the
installer's allowed write ranges or add arbitrary commands.

## Verification

Rust tests exercise the actual package parser, substitutions and upload buffer,
as well as backup/catalog/GPT/planning/execution with restricted and false-success
USB transports. The ARM emulation script executes the actual initialization,
eMMC capability handler and both read-selection instruction sequences with
storage/USB hardware calls stubbed. It compares the original and patched plug.
Mutation tests remove or invert the new checks and substitutions.

Hardware acceptance still requires a fresh MASKROM upload, capabilities with
Read LBA On, matching boundary reads and a valid tail GPT. Do not repair GPT
based on an untrusted USB read. Existing 0xCC backup files are evidence only,
not restore sources. RAM emulation is not a hardware write/readback test.

## Security boundary

This recovery Loader permits full eMMC reads while physically connected over
USB. It is not a network service or owner authentication. Someone possessing
the device and a recovery PC can read unencrypted data. The patched RAM code
does not persist across power loss.
