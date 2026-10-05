# Windows-native factory preparation

The installer matches a locally validated HAOS/Connectivity profile. The helper
downloads only pinned GHCR content and the normal app repository checkout, adds
the image using containerd's own content/metadata APIs, and modifies a disposable
data-partition copy. No daemon, VM, ARM program, first-boot hook, or CI job runs
on the customer's PC. The seven stock OS partitions are copied unchanged.

The original containerd database is not migrated or garbage-collected. Existing
image references remain unchanged. Registered compressed content is verified in
full and linked for GC; standard HAOS first-boot growth and Docker unpack provide
the installed application's filesystem. `apps.json` has the matching locally
normalized Supervisor state. The app data directory is empty, root-owned 0755.
Supervisor creates each device's options and credentials normally.

Build the Go helper with `node scripts/build.mjs` (Go >=1.26.3, Rust, Node).
`K11C_GO` selects a build-time Go executable. It is not a runtime dependency.
Native filesystem binaries/DLLs are pinned in `resources/native/tools.json`;
`native/tools.sha256` pins that manifest. The build embeds the helper hash in
the USB engine and manifest hash in the native helper. No PATH lookup or
caller-supplied executable is used at runtime.

## Filesystem utility source

e2fsprogs 1.47.4 source:
https://www.kernel.org/pub/linux/kernel/people/tytso/e2fsprogs/v1.47.4/e2fsprogs-1.47.4.tar.gz

The Windows tools were built with Cygwin GCC 14.4.0. Apply
`e2fsprogs-windows.patch`, which supplies the missing `<io.h>` declaration for
`_get_osfhandle`; no filesystem logic or feature flags are altered.
Configure in a separate build directory:

```sh
../configure --disable-nls --disable-libblkid --disable-fsck \
  --disable-e2initrd-helper --disable-mmp --disable-fuse2fs --without-udev-rules-dir
make -j4 libs
make -j4 -C debugfs
make -j4 -C e2fsck
make -j4 -C resize
```

Cygwin packages: gcc-core, make, bash, sed, grep, coreutils, gawk, diffutils,
libuuid-devel, libblkid-devel, pkg-config and their standard dependencies.
The customer's portable contains only the resulting three utilities and their
runtime DLLs. Compiler/source downloads are a developer build operation only.
Original e2fsprogs NOTICE and Cygwin/runtime notices are under `notices/native/`.

## Validation

`go test ./...` under `native/` exercises the production metadata registration,
image identity/platform, autostart state, GC graph and tool-integrity checks.
`node scripts/mutation-native.mjs` removes/reverses critical conditions and
requires the corresponding tests to fail, then restores the original sources.
The ignored Rust `actual_windows_native_production_path` test connects actual
native preparation to production composition and compares every OS byte and
the official source. Local Linux/Docker validation independently checks e2fsck,
offline image unpack/export, repository fsck, Supervisor schema and options.
Linux tools are for developer acceptance, not a dependency of the portable.
