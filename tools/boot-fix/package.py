#!/usr/bin/env python3
"""Package checked-in repair files; never accesses the board or builds a ROM."""
import hashlib
import io
import argparse
from pathlib import Path
import tarfile

HERE = Path(__file__).resolve().parent
FILES = ("boot-fix.sh", "boot-fix-lib.sh", "README.md", "VERIFICATION.md")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=Path('k11c-boot-fix-r2.tar'))
    args = parser.parse_args()
    target = args.output.resolve()
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists() or target.with_suffix(target.suffix + '.sha256').exists():
        parser.error('Output already exists; choose a new output path')
    payload = {name: (HERE / name).read_bytes() for name in FILES}
    assert all(b"\r" not in data for data in payload.values()), "Expected LF-only files"
    payload["SHA256SUMS"] = "".join(
        hashlib.sha256(data).hexdigest() + "  " + name + "\n"
        for name, data in payload.items()
    ).encode()
    with tarfile.open(target, "w", format=tarfile.USTAR_FORMAT) as archive:
        for name, data in payload.items():
            info = tarfile.TarInfo(name)
            info.mode = 0o755 if name.endswith(".sh") else 0o644
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    with tarfile.open(target) as archive:
        assert set(archive.getnames()) == set(payload)
        for name, data in payload.items():
            assert archive.extractfile(name).read() == data
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    target.with_suffix(target.suffix + '.sha256').write_text(digest + "  " + target.name + "\n", encoding="ascii")
    print("PACKAGE=" + str(target))
    print("SHA256=" + digest)
    print("PACKAGE_ROUNDTRIP_PASS files=" + str(len(payload)))


if __name__ == "__main__":
    main()
