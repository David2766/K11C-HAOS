"""Execute pinned USB plug instruction paths; no board or storage writes.

Requires unicorn==2.1.4 in --deps (build/test environment only).
The patched input must be exported by loader::tests::ram_patch_changes_only_two_audited_instructions.
"""
import argparse
import hashlib
import json
import struct
import sys
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--deps', required=True)
parser.add_argument('--patched', required=True)
args = parser.parse_args()
sys.path.insert(0, args.deps)
from unicorn import Uc, UC_ARCH_ARM64, UC_MODE_ARM, UC_HOOK_CODE
from unicorn.arm64_const import UC_ARM64_REG_SP, UC_ARM64_REG_PC, UC_ARM64_REG_LR, UC_ARM64_REG_W0, UC_ARM64_REG_W1, UC_ARM64_REG_W2, UC_ARM64_REG_X10, UC_ARM64_REG_W27, UC_ARM64_REG_W29

root = Path(__file__).resolve().parents[1]
package = (root / 'resources/loader/k11c-usb-loader-v1.23.114.bin').read_bytes()
assert hashlib.sha256(package).hexdigest() == 'b0228bbe9d3be4ea13f399df58289a82b5dfe62d1f67d75512a9ddd2d9b25454'
assert package[31] == 1
entry = struct.unpack_from('<I', package, 32)[0]
start, size = struct.unpack_from('<II', package, entry + 45)
original = package[start:start + size]
patched = Path(args.patched).read_bytes()
assert hashlib.sha256(original).hexdigest() == '10865671ac54b81bd6e59358a8635b94542ce2b65939674d0292c5a1dfac08d9'
assert hashlib.sha256(patched).hexdigest() == 'c70336dc01d4243ed10935b6b029fc46e9f129f9b6c0c5f6e7c4838428853ee7'
changed = [i for i, (a, b) in enumerate(zip(original, patched)) if a != b]
assert len(original) == len(patched) == 100352
assert changed == [0x4589, 0x975c, 0x975e, 0x975f]

def emulator(code):
    uc = Uc(UC_ARCH_ARM64, UC_MODE_ARM)
    uc.mem_map(0, 0x600000)
    uc.mem_write(0, code)
    uc.reg_write(UC_ARM64_REG_SP, 0x5ff000)
    return uc

def value(uc, address):
    return struct.unpack('<I', uc.mem_read(address, 4))[0]

def initialize(uc):
    uc.emu_start(0x971c, 0x9778, count=100)
    assert uc.reg_read(UC_ARM64_REG_PC) == 0x9778
    return value(uc, 0x459684)

def capability(uc):
    def stub(uc, address, size, _):
        if address == 0x970c:  # current storage kind: eMMC, no hardware emulation
            uc.reg_write(UC_ARM64_REG_W0, 2)
            uc.reg_write(UC_ARM64_REG_PC, uc.reg_read(UC_ARM64_REG_LR))
        elif address == 0xa4ac:  # USB send; capability bytes remain in RAM
            uc.reg_write(UC_ARM64_REG_PC, uc.reg_read(UC_ARM64_REG_LR))
    hook = uc.hook_add(UC_HOOK_CODE, stub)
    uc.emu_start(0x4554, 0x4620, count=150)
    uc.hook_del(hook)
    assert uc.reg_read(UC_ARM64_REG_PC) == 0x4620
    return bytes(uc.mem_read(0x104240, 8)).hex()

def route(uc, lba, pipeline):
    reached = []
    uc.mem_write(0x339630, struct.pack('<QI', 0x500000, lba))
    uc.mem_write(0x339644, struct.pack('<IQ', lba, 0x500000))
    uc.reg_write(UC_ARM64_REG_X10, 0x339644)
    uc.reg_write(UC_ARM64_REG_W29, 1)
    uc.reg_write(UC_ARM64_REG_W27, 512)
    def stop(uc, address, size, _):
        if address == 0x9888:
            assert uc.reg_read(UC_ARM64_REG_W0) == lba
            assert uc.reg_read(UC_ARM64_REG_W2) == 1
            reached.append('storage_read')
            uc.emu_stop()
        elif address == 0xdd78:
            assert uc.reg_read(UC_ARM64_REG_W1) == 0xcc
            assert uc.reg_read(UC_ARM64_REG_W2) == 512
            reached.append('cc_fill')
            uc.emu_stop()
    hook = uc.hook_add(UC_HOOK_CODE, stop)
    uc.emu_start(0xa190 if pipeline == 0 else 0xa094, 0, count=100)
    uc.hook_del(hook)
    assert len(reached) == 1
    return reached[0]

results = []
for name, code, expected_limit, expected_caps in [
    ('original', original, 65536, '3707000000000000'),
    ('patched', patched, 0xffffffff, '3f07000000000000'),
]:
    uc = emulator(code)
    assert initialize(uc) == expected_limit
    caps = capability(uc)
    assert caps == expected_caps, (name, caps)
    assert value(uc, 0x459684) == expected_limit, 'capability query reset limit'
    for pipeline in (0, 1):
        for lba in (1, 65535, 65536, 65537, 100352, 61071327, 61071359):
            actual = route(uc, lba, pipeline)
            expected = 'storage_read' if lba < expected_limit else 'cc_fill'
            assert actual == expected, (name, pipeline, lba, actual)
            results.append(dict(code=name, pipeline=pipeline, lba=lba, route=actual))

print(json.dumps(dict(passed=True, changed_bytes=[hex(i) for i in changed], routes=results,
                     scope='Actual instruction paths; storage and USB hardware calls stubbed. No hardware upload or write.'), indent=2))
