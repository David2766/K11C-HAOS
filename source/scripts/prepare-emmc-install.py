#!/usr/bin/env python3
"""Print image-specific U-Boot installation commands; never writes a disk or image.

For the tested 4 GiB K11C only. The input is a freshly flashed, unexpanded
K11C shim image, not a bare generic HAOS image. This changes installation GPT
handling, not HAOS payloads, OTA, or U-Boot code.
"""
import argparse
import hashlib
from pathlib import Path
import struct
import sys
import zlib

from gpt_image import ENTRY_BYTES, SECTOR, crc_header, inspect_image, require

IMAGE_RAM = 0x40000000
TAIL_RAM = 0x80000000


def prepare(path, expected_sha256, target_sectors):
    info = inspect_image(path, k11c=True)
    size = info["sectors"] * SECTOR
    require(size < TAIL_RAM - IMAGE_RAM, "Image exceeds tested non-overlapping RAM buffers")
    require(info["sectors"] + 33 <= target_sectors <= 0xFFFFFFFF, "Target capacity is too small or unsupported")
    require(info["parts"][-1][2] < target_sectors - 33, "Partition would overlap destination GPT")
    require(len(expected_sha256) == 64 and all(c in "0123456789abcdefABCDEF" for c in expected_sha256),
            "Supply the decompressed image SHA-256 from its release manifest")
    mbr = bytearray(info["mbr"])
    struct.pack_into("<I", mbr, 458, target_sectors - 1)
    primary = bytearray(info["primary"])
    struct.pack_into("<Q", primary, 32, target_sectors - 1)
    struct.pack_into("<Q", primary, 48, target_sectors - 34)
    primary = crc_header(primary)
    backup = bytearray(primary)
    struct.pack_into("<Q", backup, 24, target_sectors - 1)
    struct.pack_into("<Q", backup, 32, 1)
    struct.pack_into("<Q", backup, 72, target_sectors - 33)
    backup = crc_header(backup)
    tail = info["entries"] + backup
    digest = hashlib.sha256()
    original_crc = patched_crc = 0
    with Path(path).open("rb") as file:
        offset = 0
        while block := file.read(1024 * 1024):
            digest.update(block)
            original_crc = zlib.crc32(block, original_crc)
            if offset == 0:
                block = bytes(mbr) + primary + block[1024:]
            patched_crc = zlib.crc32(block, patched_crc)
            offset += len(block)
    require(digest.hexdigest() == expected_sha256.lower(), "Source SHA-256 does not match release manifest")
    return dict(info, new_mbr=bytes(mbr), new_primary=primary, new_backup=backup,
                tail=tail, target=target_sectors, sha256=digest.hexdigest(),
                original_crc=original_crc, patched_crc=patched_crc,
                tail_crc=zlib.crc32(tail))


def word_changes(address, original, changed):
    result = []
    for offset in range(0, len(original), 4):
        if original[offset:offset + 4] != changed[offset:offset + 4]:
            value = struct.unpack_from("<I", changed, offset)[0]
            result.append(f"mw.l {address + offset:x} {value:08x}")
    return result


def command_sections(plan):
    size = plan["sectors"] * SECTOR
    read_source = ["mmc dev 1", "mmc info", "mmc part",
                   f"mmc read {IMAGE_RAM:x} 0 {plan['sectors']:x}",
                   f"crc32 {IMAGE_RAM:x} {size:x}"]
    patch = []
    for offset, (old, new) in enumerate(zip(plan["mbr"], plan["new_mbr"])):
        if old != new:
            patch.append(f"mw.b {IMAGE_RAM + offset:x} {new:02x}")
    patch += word_changes(IMAGE_RAM + SECTOR, plan["primary"], plan["new_primary"])
    patch += [f"cp.b {IMAGE_RAM + 2 * SECTOR:x} {TAIL_RAM:x} {ENTRY_BYTES:x}",
              f"cp.b {IMAGE_RAM + SECTOR:x} {TAIL_RAM + ENTRY_BYTES:x} {SECTOR:x}"]
    patch += word_changes(TAIL_RAM + ENTRY_BYTES, plan["new_primary"], plan["new_backup"])
    patch += [f"crc32 {IMAGE_RAM:x} {size:x}", f"crc32 {TAIL_RAM:x} {len(plan['tail']):x}"]
    identity = ["mmc dev 0", "mmc info", "mmc part"]
    write = [f"mmc write {IMAGE_RAM:x} 0 {plan['sectors']:x}",
             f"mmc write {TAIL_RAM:x} {plan['target'] - 33:x} 21",
             f"mmc read {IMAGE_RAM:x} 0 {plan['sectors']:x}",
             f"crc32 {IMAGE_RAM:x} {size:x}",
             f"mmc read {TAIL_RAM:x} {plan['target'] - 33:x} 21",
             f"crc32 {TAIL_RAM:x} {len(plan['tail']):x}",
             "gpt verify mmc 0", "mmc part"]
    return read_source, patch, identity, write


def render(plan):
    sections = command_sections(plan)
    lines = ["K11C_EMMC_INSTALL_PLAN_REV=1", f"SOURCE_SHA256={plan['sha256']}",
             f"TARGET_SECTORS_DECIMAL={plan['target']} SECTOR_BYTES=512",
             "WARNING: This is a DESTRUCTIVE fresh eMMC installation, NOT an existing-install repair.",
             "No command has been sent to the board. Do not paste this entire report.",
             "Use a freshly flashed source SD. Stop before its first HAOS boot.",
             "Check every MMC operation succeeded. Stop on any error or CRC mismatch."]
    instructions = [
        ("1 READ SOURCE SD (mmc1); compare CRC before continuing", f"EXPECTED_CRC32={plan['original_crc']:08x}"),
        ("2 PREPARE BOTH GPTs IN RAM ONLY; compare both CRCs", f"EXPECTED_IMAGE_CRC32={plan['patched_crc']:08x} EXPECTED_TAIL_CRC32={plan['tail_crc']:08x}"),
        ("3 IDENTIFY DESTINATION (mmc0); do not write unless it is the intended eMMC", "Confirm exact sector capacity independently; rounded GiB alone is insufficient."),
        ("4 WRITE IMAGE AND EXPLICIT BACKUP GPT; read back both", f"EXPECTED_IMAGE_CRC32={plan['patched_crc']:08x} EXPECTED_TAIL_CRC32={plan['tail_crc']:08x}"),
    ]
    for (title, expected), commands in zip(instructions, sections):
        lines += ["", title, expected, "```text", *commands, "```"]
    lines += ["", "A gpt-verify success alone is NOT sufficient. Both readback CRCs must match.",
              "Power off fully; REMOVE SD; boot eMMC alone. Do not reset with both HAOS copies present.",
              "After HAOS first-boot expansion, on its host shell:",
              "sgdisk --print --verify /dev/mmcblk0",
              "Require No problems found. Do not accept only exit status 0.",
              "Official HAOS partitions/OTA are unchanged; this is an installation-time GPT operation."]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True, type=Path)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--target-sectors", required=True, type=int)
    args = parser.parse_args()
    try:
        report = render(prepare(args.image, args.sha256, args.target_sectors))
    except (ValueError, OSError) as error:
        parser.exit(1, f"FAIL: {error}\n")
    sys.stdout.write(report)


if __name__ == "__main__":
    main()
