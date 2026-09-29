#!/usr/bin/env python3
"""Read-only, paired GPT validation for 512-byte K11C installation images.

Do not substitute a successful sgdisk/gpt-verify exit status for this check:
individually valid GPT headers can describe different disks.
"""
import argparse
from pathlib import Path
import struct
import uuid
import zlib

SECTOR = 512
ENTRY_BYTES = 16384
LABELS = ["hassos-boot", "hassos-kernel0", "hassos-system0", "hassos-kernel1",
          "hassos-system1", "hassos-bootstate", "hassos-overlay", "hassos-data"]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def crc_header(header):
    result = bytearray(header)
    size = struct.unpack_from("<I", result, 12)[0]
    require(92 <= size <= SECTOR, "Unsupported GPT header size")
    result[16:20] = bytes(4)
    struct.pack_into("<I", result, 16, zlib.crc32(result[:size]))
    return bytes(result)


def read_at(file, offset, count):
    file.seek(offset)
    data = file.read(count)
    require(len(data) == count, "Short image read")
    return data


def header_fields(raw, lba, sectors):
    require(raw[:8] == b"EFI PART", "GPT signature missing")
    require(struct.unpack_from("<I", raw, 8)[0] == 0x10000, "GPT revision changed")
    require(raw[16:20] == crc_header(raw)[16:20], "GPT header CRC mismatch")
    own, other, first, last = struct.unpack_from("<4Q", raw, 24)
    entries_lba, count, size, entries_crc = struct.unpack_from("<QIII", raw, 72)
    require(own == lba and other == (sectors - 1 if lba == 1 else 1),
            "GPT reciprocal header locations differ")
    require(count == 128 and size == 128, "Expected 128 GPT entries of 128 bytes")
    require(entries_lba == (2 if lba == 1 else sectors - 33), "Unexpected GPT entry location")
    require(34 <= first <= last <= sectors - 34, "GPT usable bounds overlap metadata")
    require(raw[20:24] == bytes(4), "GPT reserved field is nonzero")
    return first, last, entries_lba, entries_crc


def inspect_image(path, *, k11c=False):
    path = Path(path)
    require(path.is_file(), "Expected a regular image file")
    length = path.stat().st_size
    require(length % SECTOR == 0 and length >= 68 * SECTOR, "Image size is not sector aligned")
    sectors = length // SECTOR
    with path.open("rb") as file:
        mbr = read_at(file, 0, SECTOR)
        primary = read_at(file, SECTOR, SECTOR)
        backup = read_at(file, (sectors - 1) * SECTOR, SECTOR)
        p = header_fields(primary, 1, sectors)
        b = header_fields(backup, sectors - 1, sectors)
        require(primary[56:72] == backup[56:72], "Primary/backup disk GUID mismatch")
        require(primary[8:16] == backup[8:16] and p[:2] == b[:2], "GPT header policy differs")
        entries = read_at(file, p[2] * SECTOR, ENTRY_BYTES)
        backup_entries = read_at(file, b[2] * SECTOR, ENTRY_BYTES)
    require(zlib.crc32(entries) == p[3] and zlib.crc32(backup_entries) == b[3],
            "GPT partition-array CRC mismatch")
    require(entries == backup_entries, "Primary/backup partition arrays differ")
    require(mbr[510:512] == b"\x55\xaa" and mbr[450] == 0xEE, "Protective MBR missing")
    require(struct.unpack_from("<II", mbr, 454) == (1, min(sectors - 1, 0xFFFFFFFF)),
            "Protective MBR capacity differs")
    require(mbr[462:510] == bytes(48), "Hybrid MBR is not supported")
    parts = []
    for index in range(128):
        entry = entries[index * 128:(index + 1) * 128]
        if entry[:16] == bytes(16):
            continue
        start, end = struct.unpack_from("<QQ", entry, 32)
        require(p[0] <= start <= end <= p[1], "Partition outside usable bounds")
        name = entry[56:128].decode("utf-16-le").split("\0", 1)[0]
        parts.append((index + 1, start, end, name))
    ordered = sorted(parts, key=lambda row: row[1])
    require(all(a[2] < b[1] for a, b in zip(ordered, ordered[1:])), "Partitions overlap")
    if k11c:
        require([row[0] for row in parts] == list(range(1, 9)) and
                [row[3] for row in parts] == LABELS, "Expected the eight HAOS partitions")
        require(parts[0][1] == 34816, "K11C U-Boot reservation is missing")
    return {"sectors": sectors, "mbr": mbr, "primary": primary, "backup": backup,
            "entries": entries, "parts": parts,
            "guid": str(uuid.UUID(bytes_le=primary[56:72])).upper()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path)
    parser.add_argument("--k11c", action="store_true")
    args = parser.parse_args()
    try:
        result = inspect_image(args.image, k11c=args.k11c)
    except (ValueError, OSError) as error:
        parser.exit(1, f"FAIL: {error}\n")
    print(f"GPT_PAIR_PASS sectors={result['sectors']} guid={result['guid']} crc=both arrays=identical")


if __name__ == "__main__":
    main()
