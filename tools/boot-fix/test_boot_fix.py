#!/usr/bin/env python3
"""Run production repair functions with real sgdisk against disposable sparse images.

No host block device, mount, network connection, reboot, or source-tree write.
"""
import os
from pathlib import Path
import shlex
import struct
import subprocess
import tempfile
import uuid
import zlib

HERE = Path(__file__).resolve().parent
LIB = HERE / "boot-fix-lib.sh"
GUID = "353A3772-554F-408B-A80B-05747613DEA7"
OLD_BACKUP_GUID = "66620000-0000-452A-8000-67C90000290D"
SECTORS = 61071360
PARTS = [
    (34816, 100351, "hassos-boot", "EF00"),
    (100352, 149503, "hassos-kernel0", "8300"),
    (149504, 673791, "hassos-system0", "8300"),
    (673792, 722943, "hassos-kernel1", "8300"),
    (722944, 1247231, "hassos-system1", "8300"),
    (1247232, 1263615, "hassos-bootstate", "8300"),
    (1263616, 1460223, "hassos-overlay", "8300"),
    (1460224, 61071326, "hassos-data", "8300"),
]
CMDLINE = b"console=ttyS0 console=tty0 test.keep=yes\n"
EXPECTED = b"console=ttyS0,1500000n8 console=tty0 test.keep=yes\n"


def run(args, **kwargs):
    return subprocess.run(args, capture_output=True, text=True, check=True, **kwargs)


def fixture(root, main_guid=GUID):
    disk = root / "emmc.img"
    with disk.open("wb") as file:
        file.truncate(SECTORS * 512)
    args = ["sgdisk", "--clear", "--disk-guid=" + main_guid]
    for n, (start, end, name, kind) in enumerate(PARTS, 1):
        args += [f"--new={n}:{start}:{end}", f"--change-name={n}:{name}", f"--typecode={n}:{kind}"]
    run(args + [str(disk)])
    with disk.open("r+b") as file:
        # Representative existing raw U-Boot bytes and partition-data sentinels.
        file.seek(64 * 512)
        file.write(bytes(range(256)) * 4096)
        for n, (start, end, _, _) in enumerate(PARTS, 1):
            for sector in (start, end):
                file.seek(sector * 512)
                file.write(bytes([n]) * 512)
        file.seek((SECTORS - 1) * 512)
        header = bytearray(file.read(512))
        header[56:72] = uuid.UUID(OLD_BACKUP_GUID).bytes_le
        header[16:20] = b"\0" * 4
        size = struct.unpack_from("<I", header, 12)[0]
        struct.pack_into("<I", header, 16, zlib.crc32(header[:size]))
        file.seek((SECTORS - 1) * 512)
        file.write(header)
    cmd = root / "cmdline.txt"
    cmd.write_bytes(CMDLINE)
    return disk, cmd, root / "backup"


def call(lib, state, action, ok=True):
    disk, cmd, backup = state
    script = "\n".join([
        ". " + shlex.quote(str(lib)),
        "K11C_DEVICE=" + shlex.quote(str(disk)),
        "K11C_CMDLINE=" + shlex.quote(str(cmd)),
        "K11C_BACKUP=" + shlex.quote(str(backup)),
        action,
    ])
    proc = subprocess.run(["/bin/sh", "-c", script], capture_output=True, text=True)
    if ok and proc.returncode:
        raise AssertionError(proc.stdout + proc.stderr)
    if not ok:
        assert proc.returncode != 0, "unsafe input unexpectedly accepted"
    return proc


def read_at(disk, offset, count):
    with disk.open("rb") as file:
        file.seek(offset)
        data = file.read(count)
    assert len(data) == count
    return data


def protected_bytes(disk):
    return (
        read_at(disk, 0, 34816 * 512),
        read_at(disk, (SECTORS - 33) * 512, 32 * 512),
        tuple(read_at(disk, sector * 512, 512)
              for part in PARTS for sector in part[:2]),
    )


def roundtrip(lib, root):
    state = fixture(root)
    disk, cmd, backup = state
    before = protected_bytes(disk)
    header_before = read_at(disk, (SECTORS - 1) * 512, 512)
    call(lib, state, "backup_state")
    call(lib, state, "repair_gpt")
    assert "No problems found." in run(["sgdisk", "--verify", str(disk)]).stdout
    assert protected_bytes(disk) == before, "MBR, primary GPT, U-Boot, entries or data changed"
    header_after = read_at(disk, (SECTORS - 1) * 512, 512)
    assert header_after[56:72] == uuid.UUID(GUID).bytes_le
    allowed = set(range(16, 20)) | set(range(56, 72))
    assert all(a == b or n in allowed for n, (a, b) in enumerate(zip(header_before, header_after)))
    assert "writes=0" in call(lib, state, "repair_gpt").stdout
    assert cmd.read_bytes() == CMDLINE, "GPT-only operation changed console settings"
    assert "K11C_GPT_CHECK_PASS" in call(lib, state, "dmesg() { :; }; check_gpt_only").stdout
    call(lib, state, "set_console")
    assert cmd.read_bytes() == EXPECTED, "console token or unrelated arguments wrong"
    assert "already_set=1" in call(lib, state, "set_console").stdout
    call(lib, state, "restore_console")
    assert cmd.read_bytes() == CMDLINE
    call(lib, state, "restore_console")
    assert protected_bytes(disk) == before
    return state


def wrong_guid(lib, root):
    state = fixture(root, "12345678-1234-4234-8234-123456789ABC")
    before = protected_bytes(state[0])
    call(lib, state, "backup_state", ok=False)
    assert protected_bytes(state[0]) == before


def wrong_layout(lib, root):
    state = fixture(root)
    run(["sgdisk", "--delete=8", str(state[0])])
    before = protected_bytes(state[0])
    call(lib, state, "backup_state", ok=False)
    assert protected_bytes(state[0]) == before


def changed_backup(lib, root):
    state = fixture(root)
    call(lib, state, "backup_state")
    (state[2] / "cmdline.before.txt").write_bytes(b"tampered\n")
    before = protected_bytes(state[0])
    call(lib, state, "repair_gpt", ok=False)
    assert protected_bytes(state[0]) == before


def changed_cmdline(lib, root):
    state = fixture(root)
    call(lib, state, "backup_state")
    state[1].write_bytes(b"console=ttyS0 console=tty0 user.new=yes\n")
    current = state[1].read_bytes()
    call(lib, state, "set_console", ok=False)
    assert state[1].read_bytes() == current


def ambiguous_console(lib, root):
    state = fixture(root)
    state[1].write_bytes(b"console=ttyS0 console=ttyS0,115200n8 console=tty0\n")
    call(lib, state, "backup_state")
    call(lib, state, "set_console", ok=False)


def whitespace_console(lib, root):
    state = fixture(root)
    original = b"keep=x\tconsole=ttyS0\tconsole=tty0\n"
    state[1].write_bytes(original)
    call(lib, state, "backup_state")
    call(lib, state, "set_console")
    assert state[1].read_bytes() == original.replace(b"console=ttyS0", b"console=ttyS0,1500000n8")
    call(lib, state, "restore_console")
    assert state[1].read_bytes() == original


def later_restore_edits(lib, root):
    state = fixture(root)
    call(lib, state, "backup_state")
    call(lib, state, "set_console")
    state[1].write_bytes(EXPECTED.rstrip() + b" user.later=yes\n")
    current = state[1].read_bytes()
    call(lib, state, "restore_console", ok=False)
    assert state[1].read_bytes() == current


def changed_primary(lib, root):
    state = fixture(root)
    call(lib, state, "backup_state")
    run(["sgdisk", "--attributes=2:set:2", str(state[0])])
    before = protected_bytes(state[0])
    call(lib, state, "repair_gpt", ok=False)
    assert protected_bytes(state[0]) == before


def pair_guid_mismatch(lib, root):
    state = fixture(root)
    call(lib, state, "backup_state")
    call(lib, state, 'check_gpt_pair "$K11C_BACKUP/pair-test"', ok=False)


def pair_entries_mismatch(lib, root):
    state = fixture(root)
    call(lib, state, "backup_state")
    call(lib, state, "repair_gpt")
    disk = state[0]
    with disk.open("r+b") as file:
        file.seek((SECTORS - 33) * 512)
        entries = bytearray(file.read(32 * 512))
        entries[48] ^= 4
        file.seek((SECTORS - 33) * 512)
        file.write(entries)
        header = bytearray(file.read(512))
        struct.pack_into("<I", header, 88, zlib.crc32(entries))
        header[16:20] = bytes(4)
        struct.pack_into("<I", header, 16, zlib.crc32(header[:92]))
        file.seek((SECTORS - 1) * 512)
        file.write(header)
    call(lib, state, 'check_gpt_pair "$K11C_BACKUP/pair-test"', ok=False)
    before = protected_bytes(disk)
    call(lib, state, "repair_gpt", ok=False)
    assert protected_bytes(disk) == before


def actual_board_metadata(lib, root):
    captured = HERE.parents[2] / "artifacts/K11C/boot-fix-r1/k11c-boot-backup-r1"
    assert captured.is_dir(), "Original board metadata capture required for this local test"
    state = fixture(root)
    disk, cmd, backup = state
    with disk.open("r+b") as file:
        for offset, name in ((0, "gpt-primary.bin"), (34 * 512, "uboot-reserved.bin"),
                             ((SECTORS - 33) * 512, "gpt-backup.bin")):
            file.seek(offset)
            file.write((captured / name).read_bytes())
    before = protected_bytes(disk)
    header_before = read_at(disk, (SECTORS - 1) * 512, 512)
    call(lib, state, "backup_state")
    call(lib, state, "repair_gpt")
    call(lib, state, "dmesg() { :; }; check_gpt_only")
    assert protected_bytes(disk) == before
    after = read_at(disk, (SECTORS - 1) * 512, 512)
    assert after[56:72] == uuid.UUID(GUID).bytes_le
    assert all(a == b or i in set(range(16, 20)) | set(range(56, 72))
               for i, (a, b) in enumerate(zip(header_before, after)))
    assert cmd.read_bytes() == CMDLINE


def entrypoint_rejects_wrong_host():
    # Invoke the unmodified production entry point, not just its library.
    # This suite is intended for a non-root development host, never the board.
    assert os.geteuid() != 0, "Run the local fixture suite as an unprivileged user"
    proc = subprocess.run([
        "/bin/sh", str(HERE / "boot-fix.sh"), "backup",
        "/mnt/data/supervisor/share/k11c-boot-backup-local-negative",
    ], capture_output=True, text=True)
    assert proc.returncode != 0 and "Run on HAOS host serial" in proc.stderr
    print("PASS production_entrypoint_rejects_nonroot", flush=True)


def main():
    entrypoint_rejects_wrong_host()
    run(["/bin/sh", "-n", str(LIB)])
    run(["/bin/sh", "-n", str(HERE / "boot-fix.sh")])
    source = LIB.read_text()
    cases = [roundtrip, wrong_guid, wrong_layout, changed_backup,
             changed_cmdline, ambiguous_console, whitespace_console,
             later_restore_edits, changed_primary, pair_guid_mismatch,
             pair_entries_mismatch, actual_board_metadata]
    with tempfile.TemporaryDirectory(prefix="k11c-boot-fix-") as temp:
        root = Path(temp)
        for test in cases:
            work = root / test.__name__
            work.mkdir()
            test(LIB, work)
            print("PASS", test.__name__, flush=True)
        mutations = [
            ("disk_identity_guard_removed", wrong_guid,
             'grep -Fqx "label-id: $K11C_GUID" "$1" || fail', ': || fail'),
            ("layout_guard_inverted", roundtrip, 'if (n != 8)', 'if (n == 8)'),
            ("checksum_guard_removed", changed_backup,
             '(cd "$K11C_BACKUP" && sha256sum -c SHA256SUMS) || fail', ': || fail'),
            ("later_cmdline_guard_removed", changed_cmdline,
             'same "$K11C_CMDLINE" "$K11C_BACKUP/cmdline.before.txt"', ': '),
            ("wrong_console_speed", roundtrip, '1500000n8', '115200n8'),
            ("pair_GUID_comparison_removed", pair_guid_mismatch,
             'same "$K11C_PAIR.guid-primary" "$K11C_PAIR.guid-backup"', ': '),
            ("pair_entry_comparison_removed", pair_entries_mismatch,
             'same "$K11C_PAIR.entries-primary" "$K11C_PAIR.entries-backup"', ': '),
            ("GPT_write_removed", roundtrip,
             'sgdisk --disk-guid="$K11C_GUID" "$K11C_DEVICE"', ':'),
        ]
        for name, test, old, new in mutations:
            assert old in source
            mutant = root / (name + ".sh")
            mutant.write_text(source.replace(old, new))
            work = root / name
            work.mkdir()
            try:
                test(mutant, work)
            except (AssertionError, subprocess.CalledProcessError):
                print("MUTATION_REJECTED", name, flush=True)
            else:
                raise AssertionError("mutation escaped: " + name)
    print(f"LOCAL_PASS cases={len(cases)} mutations={len(mutations)} entrypoint_negative=1 real_sgdisk=1 hardware_applied=0", flush=True)


if __name__ == "__main__":
    main()
