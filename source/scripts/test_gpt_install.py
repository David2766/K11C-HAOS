#!/usr/bin/env python3
"""Run the generated U-Boot memory/MMC operations on disposable disk files.

No board, block device, network, image build or OS compilation is involved.
"""
import hashlib
import argparse
import importlib.util
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import uuid
import zlib

import gpt_image as gpt

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("installer", HERE / "prepare-emmc-install.py")
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
TARGET = 61071360
OLD_GUID = uuid.UUID("66620000-0000-452A-8000-67C90000290D").bytes_le


def run(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, text=True, **kwargs)


def source_fixture(path):
    with path.open("wb") as file:
        file.truncate(64 * 1024 * 1024)
    args = ["sgdisk", "--clear", "--disk-guid=353A3772-554F-408B-A80B-05747613DEA7"]
    for index, label in enumerate(gpt.LABELS):
        first = 34816 + index * 2048
        args += [f"--new={index + 1}:{first}:{first + 2047}", f"--change-name={index + 1}:{label}"]
    run(args + [str(path)])
    with path.open("r+b") as file:
        file.seek(34 * 512)
        file.write(bytes(range(256)) * ((34816 - 34) * 2))
        for index in range(8):
            file.seek((34816 + index * 2048) * 512)
            file.write(bytes([index + 1]) * (2048 * 512))


def put(path, offset, value):
    with path.open("r+b") as file:
        file.seek(offset)
        file.write(value)


def get(path, offset, length):
    with path.open("rb") as file:
        return gpt.read_at(file, offset, length)


def must_reject(action, expected):
    try:
        action()
    except ValueError as error:
        assert expected in str(error), str(error)
    else:
        raise AssertionError("invalid state was accepted: " + expected)


def emitted_commands(plan):
    inside = False
    for line in installer.render(plan).splitlines():
        if line == "```text":
            inside = True
        elif line == "```":
            inside = False
        elif inside:
            yield line


def simulate(source, disk, plan, *, omit_tail=False):
    """Interpret emitted commands, including readbacks; compare all four CRCs."""
    with disk.open("wb") as file:
        file.truncate(plan["target"] * 512)
    run(["sgdisk", "--clear", "--disk-guid=" + str(uuid.UUID(bytes_le=OLD_GUID)), str(disk)])
    memory = {installer.IMAGE_RAM: bytearray(source.stat().st_size),
              installer.TAIL_RAM: bytearray(33 * 512)}
    selected = None
    crcs = []

    def area(address, count):
        for base, data in memory.items():
            if base <= address and address + count <= base + len(data):
                return memoryview(data)[address - base:address - base + count]
        raise AssertionError("out-of-bounds U-Boot RAM access")

    for line in emitted_commands(plan):
        args = line.split()
        if args[:2] == ["mmc", "dev"]:
            selected = source if args[2] == "1" else disk
        elif args[:2] in (["mmc", "info"], ["mmc", "part"]):
            continue
        elif args[0] == "mmc":
            address, lba, count = (int(v, 16) for v in args[2:])
            view = area(address, count * 512)
            if args[1] == "read":
                view[:] = get(selected, lba * 512, count * 512)
            elif args[1] == "write":
                assert selected == disk, "attempted SD write"
                if not (omit_tail and address == installer.TAIL_RAM):
                    put(selected, lba * 512, view)
            else:
                raise AssertionError(line)
        elif args[0] in ("mw.b", "mw.l"):
            width = 1 if args[0] == "mw.b" else 4
            area(int(args[1], 16), width)[:] = int(args[2], 16).to_bytes(width, "little")
        elif args[0] == "cp.b":
            src, dest, count = (int(v, 16) for v in args[1:])
            area(dest, count)[:] = bytes(area(src, count))
        elif args[0] == "crc32":
            address, length = (int(v, 16) for v in args[1:])
            crcs.append(zlib.crc32(area(address, length)))
        elif args == ["gpt", "verify", "mmc", "0"]:
            # Historical U-Boot only validates headers individually. Do not
            # let this simulator strengthen that command and hide the bug.
            continue
        else:
            raise AssertionError("unimplemented command: " + line)
    return crcs


def sha(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def invariants(source, disk, plan):
    pair = gpt.inspect_image(disk, k11c=True)
    assert pair["entries"] == plan["entries"], "partition identity or geometry changed"
    # Covers ALL U-Boot bytes, ALL eight partition payloads, padding and the
    # old image-tail GPT. Only the first two sectors and new disk tail differ.
    with source.open("rb") as original, disk.open("rb") as written:
        original.seek(1024)
        written.seek(1024)
        while chunk := original.read(1024 * 1024):
            assert written.read(len(chunk)) == chunk, "protected image content changed"
    assert get(disk, 0, 512) == plan["new_mbr"]
    assert get(disk, 512, 512) == plan["new_primary"]
    assert get(disk, (plan["target"] - 33) * 512, 33 * 512) == plan["tail"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--original", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--uboot", type=Path)
    args = parser.parse_args()
    assert all((args.original, args.candidate, args.uboot)) or not any((args.original, args.candidate, args.uboot))
    with tempfile.TemporaryDirectory(prefix="k11c-gpt-install-") as temp:
        root = Path(temp)
        source = root / "source.img"
        source_fixture(source)
        digest = sha(source)
        plan = installer.prepare(source, digest, TARGET)
        disk = root / "destination.img"
        expected = [plan["original_crc"], plan["patched_crc"], plan["tail_crc"],
                    plan["patched_crc"], plan["tail_crc"]]
        assert simulate(source, disk, plan) == expected
        invariants(source, disk, plan)
        assert sha(source) == digest, "source image was modified"
        report = run(["sgdisk", "--verify", str(disk)]).stdout
        assert "No problems found." in report
        run([sys.executable, str(HERE / "gpt_image.py"), "--k11c", str(disk)])
        cli = run([sys.executable, str(HERE / "prepare-emmc-install.py"), "--image", str(source),
                   "--sha256", digest, "--target-sectors", str(TARGET)])
        assert cli.stdout == installer.render(plan)
        print("PASS generated_commands readback_crcs all_payload_bytes source_readonly production_cli", flush=True)

        # Execute the very sfdisk binary called by HAOS's first-boot expander.
        sysroot = Path("/home/user/work/haos-18.2/output/target")
        result = run(["qemu-aarch64-static", "-L", str(sysroot), str(sysroot / "sbin/sfdisk"),
                      "--no-reread", "--no-tell-kernel", "-N", "8", str(disk)], input=", +\n")
        expanded = gpt.inspect_image(disk, k11c=True)
        assert expanded["guid"] == plan["guid"] and expanded["parts"][-1][2] == TARGET - 34
        assert expanded["entries"][:7 * 128] == plan["entries"][:7 * 128]
        print("PASS actual_HAOS_aarch64_sfdisk_firstboot_expansion", flush=True)

        must_reject(lambda: installer.prepare(source, "0" * 64, TARGET), "SHA-256")
        must_reject(lambda: installer.prepare(source, digest, 100), "Target capacity")
        print("PASS wrong_source_hash_and_capacity_rejected", flush=True)

        omitted = root / "omitted-tail.img"
        crcs = simulate(source, omitted, plan, omit_tail=True)
        assert crcs != expected
        must_reject(lambda: gpt.inspect_image(omitted), "disk GUID mismatch")
        print("MUTATION_REJECTED explicit_backup_write_removed", flush=True)

        bad = root / "bad-source.img"
        shutil.copyfile(source, bad)
        tail_offset = bad.stat().st_size - 512
        header = bytearray(get(bad, tail_offset, 512))
        header[56:72] = OLD_GUID
        put(bad, tail_offset, gpt.crc_header(header))
        assert run(["sgdisk", "--verify", str(bad)]).returncode == 0  # regression trigger
        must_reject(lambda: installer.prepare(bad, sha(bad), TARGET), "disk GUID mismatch")
        print("PASS independently_valid_but_inconsistent_GPT_rejected", flush=True)
        original_require = gpt.require
        def omit_guid_guard(condition, message):
            if message != "Primary/backup disk GUID mismatch":
                original_require(condition, message)
        gpt.require = omit_guid_guard
        try:
            must_reject(lambda: installer.prepare(bad, sha(bad), TARGET), "disk GUID mismatch")
        except AssertionError:
            print("MUTATION_REJECTED GUID_comparison_removed", flush=True)
        else:
            raise AssertionError("GUID mutation escaped")
        finally:
            gpt.require = original_require

        shutil.copyfile(source, bad)
        put(bad, 512 + 16, b"\0\0\0\0")
        must_reject(lambda: gpt.inspect_image(bad), "header CRC")
        shutil.copyfile(source, bad)
        put(bad, 1024 + 100, b"broken")
        must_reject(lambda: gpt.inspect_image(bad), "partition-array CRC")
        print("PASS corrupt_header_and_entries_rejected", flush=True)
        if args.candidate:
            validator = HERE / "validate-shim-image.sh"
            validation_args = ["--original", str(args.original), "--candidate", str(args.candidate),
                               "--uboot", str(args.uboot)]
            run(["bash", str(validator), *validation_args])
            print("PASS real_release_complete_production_validator", flush=True)
            altered = root / "altered-release.img"
            run(["cp", "--reflink=auto", "--sparse=always", str(args.candidate), str(altered)])
            end = altered.stat().st_size - 512
            header = bytearray(get(altered, end, 512))
            header[56:72] = OLD_GUID
            put(altered, end, gpt.crc_header(header))
            validation_args[3] = str(altered)
            negative = subprocess.run(["bash", str(validator), *validation_args], capture_output=True, text=True)
            assert negative.returncode and "disk GUID mismatch" in negative.stderr
            print("PASS real_release_valid_but_mismatched_backup_rejected", flush=True)
            source_code = validator.read_text()
            guard = 'python3 "$(dirname "${BASH_SOURCE[0]}")/gpt_image.py" --k11c "$CANDIDATE"'
            assert guard in source_code
            mutant = root / "validator-guard-removed.sh"
            mutant.write_text(source_code.replace(guard, ":"))
            shutil.copyfile(HERE / "gpt_image.py", root / "gpt_image.py")
            escaped = subprocess.run(["bash", str(mutant), *validation_args], capture_output=True, text=True)
            assert escaped.returncode == 0, escaped.stdout + escaped.stderr
            print("MUTATION_REJECTED production_GPT_pair_call_removed (negative test would fail)", flush=True)
    print("K11C_GPT_INSTALL_LOCAL_PASS board_written=0 HAOS_payload_modified=0", flush=True)


if __name__ == "__main__":
    main()
