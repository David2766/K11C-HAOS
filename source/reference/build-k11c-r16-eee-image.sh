#!/usr/bin/env bash
set -euo pipefail

port_root=/mnt/c/rom/HAOS/K11C-port
canonical_dts=$port_root/device-tree/linux/rk3566-kickpi-k11c.dts
variant_dts=$port_root/diagnostics/vqmmc-ab/rk3566-kickpi-k11c-no-vqmmc.dts
uboot_dtsi=$port_root/device-tree/u-boot/rk3566-kickpi-k11c-u-boot.dtsi
linux_tree=/home/user/work/haos-18.2/output/build/linux-6.18.39
uboot_tree=/home/user/work/k11c-port-build/u-boot-2026.04
uboot_hook=$uboot_tree/board/hardkernel/odroid_m1s/k11c.c
host_bin=/home/user/work/haos-18.2/output/host/bin
vendor_rkbin=/mnt/c/rom/HAOS/vendor-k11c/sdk-20260515/rkbin/bin/rk35
ddr_blob=$vendor_rkbin/rk3566_ddr_1056MHz_v1.23.bin
bl31_blob=$vendor_rkbin/rk3568_bl31_v1.44.elf
base_image=/mnt/c/rom/HAOS/artifacts/K11C/haos_generic-aarch64-18.2-k11c-shim-r3.img
r14_dtb=/mnt/c/rom/HAOS/artifacts/K11C/rk3566-kickpi-k11c-sdio-no-vqmmc-ab-r14.dtb
output_dir=/mnt/c/rom/HAOS/artifacts/K11C
output_image=$output_dir/haos_generic-aarch64-18.2-k11c-lan-eee-r16.img
output_uboot=$output_dir/u-boot-rockchip-k11c-lan-eee-r16.bin
output_dtb=$output_dir/rk3566-kickpi-k11c-lan-eee-r16.dtb
output_uboot_only_sd=$output_dir/k11c-r16-eee-uboot-only-sd.img
manifest=$output_image.manifest.txt
record_dir=/home/user/work/k11c-port-build/lan-eee-r16

expected_base_sha256=1a40b532a2f1248783d1b0fd36d5946bd8ce60612b0c12d45bbe796f66e86072
expected_canonical_dts_sha256=1e917dc5df157ffaf9d875d0f385aabae41975ca58b0d0b14ded13269a9b72bf
expected_uboot_source_dts_sha256=438dad9c6ab2a4582f264593570dc87e886d844da61d1f4a1b10b237c110998f
expected_uboot_dtsi_sha256=5a9f6a08aeb7148f9088d2151cc2b3346140bd62984e4e5845c5aa45eb1e0a0b
expected_uboot_hook_sha256=c2d8c9f0778f2f8ac0bf4b33b3bf4060a3feab21cbd742fc7d569432e9c91088
expected_r14_dtb_sha256=ebfbe5417669846005641780226133e09d61f29757629bb32f6b3cade3ef7240

sector_size=512
shim_offset=32768
first_partition_lba=34816
jobs=${JOBS:-$(nproc)}

# Keep the version string and FIT timestamps stable for this versioned artifact.
export SOURCE_DATE_EPOCH=1788537600
export KBUILD_BUILD_TIMESTAMP='2026-09-05 00:00:00 +0800'
export KBUILD_BUILD_USER=k11c
export KBUILD_BUILD_HOST=builder
export KBUILD_BUILD_VERSION=1

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

require_file() {
	[[ -s "$1" ]] || fail "required file is missing or empty: $1"
}

expect_eq() {
	[[ "$1" == "$2" ]] || fail "$3: expected '$2', got '$1'"
}

expect_absent() {
	local dtb=$1 node=$2 property=$3
	if "$host_bin/fdtget" "$dtb" "$node" "$property" >/dev/null 2>&1; then
		fail "$node/$property must be absent"
	fi
}

assert_wifi_pinctrl() {
	local dtb=$1
	local wifi_enable sclktx lrcktx sdi sdo pcfg_none pcfg_smt actual
	wifi_enable=$("$host_bin/fdtget" -t x "$dtb" \
		/pinctrl/sdio-pwrseq/wifi-enable-h phandle)
	sclktx=$("$host_bin/fdtget" -t x "$dtb" \
		/pinctrl/i2s2/i2s2m0-sclktx phandle)
	lrcktx=$("$host_bin/fdtget" -t x "$dtb" \
		/pinctrl/i2s2/i2s2m0-lrcktx phandle)
	sdi=$("$host_bin/fdtget" -t x "$dtb" \
		/pinctrl/i2s2/i2s2m0-sdi phandle)
	sdo=$("$host_bin/fdtget" -t x "$dtb" \
		/pinctrl/i2s2/i2s2m0-sdo phandle)
	pcfg_none=$("$host_bin/fdtget" -t x "$dtb" /pinctrl/pcfg-pull-none phandle)
	pcfg_smt=$("$host_bin/fdtget" -t x "$dtb" /pinctrl/pcfg-pull-none-smt phandle)
	actual=$("$host_bin/fdtget" -t x "$dtb" /sdio-pwrseq pinctrl-0)
	expect_eq "$actual" "$wifi_enable $sclktx $lrcktx $sdi $sdo" \
		"wireless reset and PCM pinctrl sequence"
	expect_eq "$("$host_bin/fdtget" -t x "$dtb" \
		/pinctrl/i2s2/i2s2m0-sclktx rockchip,pins)" \
		"2 12 1 $pcfg_smt" "wireless PCM bit-clock pin"
	expect_eq "$("$host_bin/fdtget" -t x "$dtb" \
		/pinctrl/i2s2/i2s2m0-lrcktx rockchip,pins)" \
		"2 13 1 $pcfg_smt" "wireless PCM frame-clock pin"
	expect_eq "$("$host_bin/fdtget" -t x "$dtb" \
		/pinctrl/i2s2/i2s2m0-sdi rockchip,pins)" \
		"2 15 1 $pcfg_none" "wireless PCM input pin"
	expect_eq "$("$host_bin/fdtget" -t x "$dtb" \
		/pinctrl/i2s2/i2s2m0-sdo rockchip,pins)" \
		"2 14 1 $pcfg_none" "wireless PCM output pin"
	expect_eq "$("$host_bin/fdtget" "$dtb" /i2s@fe420000 status)" disabled \
		"I2S2 controller status"
}

assert_lan_dtb() {
	local dtb=$1
	expect_eq "$("$host_bin/fdtget" "$dtb" /ethernet@fe010000 status)" okay \
		"Ethernet status"
	expect_eq "$("$host_bin/fdtget" "$dtb" /ethernet@fe010000 phy-mode)" rgmii \
		"Ethernet PHY mode"
	expect_eq "$("$host_bin/fdtget" "$dtb" /ethernet@fe010000 clock_in_out)" output \
		"Ethernet clock direction"
	expect_eq "$("$host_bin/fdtget" "$dtb" /ethernet@fe010000 tx_delay)" 66 \
		"Ethernet TX delay"
	expect_eq "$("$host_bin/fdtget" "$dtb" /ethernet@fe010000 rx_delay)" 57 \
		"Ethernet RX delay"
	expect_eq "$("$host_bin/fdtget" "$dtb" \
		/ethernet@fe010000/mdio/ethernet-phy@0 reg)" 0 "Ethernet PHY address"
	"$host_bin/fdtget" -p "$dtb" /ethernet@fe010000/mdio/ethernet-phy@0 | \
		grep -Fxq reset-gpios || fail "Ethernet PHY reset GPIO is missing"
	for property in eee-broken-100tx eee-broken-1000t; do
		"$host_bin/fdtget" -p "$dtb" \
			/ethernet@fe010000/mdio/ethernet-phy@0 | \
			grep -Fxq "$property" || fail "Ethernet PHY $property is missing"
	done
}

compile_linux_dtb() {
	local source=$1 output=$2 preprocessed=$3
	cpp -nostdinc \
		-I "$linux_tree/arch/arm64/boot/dts/rockchip" \
		-I "$linux_tree/scripts/dtc/include-prefixes" \
		-I "$linux_tree/include" \
		-undef -D__DTS__ -x assembler-with-cpp \
		"$source" > "$preprocessed"
	"$linux_tree/scripts/dtc/dtc" -I dts -O dtb -o "$output" "$preprocessed"
}

for command in cmp cpp dd grep install make sha256sum sgdisk stat truncate; do
	command -v "$command" >/dev/null || fail "missing command: $command"
done
for file in "$canonical_dts" "$variant_dts" "$uboot_dtsi" "$uboot_hook" \
	"$ddr_blob" "$bl31_blob" "$base_image" "$r14_dtb" \
	"$linux_tree/scripts/dtc/dtc" "$host_bin/fdtget" "$host_bin/fdtput"; do
	require_file "$file"
done

expect_eq "$(sha256sum "$base_image" | cut -d' ' -f1)" \
	"$expected_base_sha256" "r3 base image hash"
expect_eq "$(sha256sum "$canonical_dts" | cut -d' ' -f1)" \
	"$expected_canonical_dts_sha256" "canonical DTS hash"
expect_eq "$(sha256sum "$uboot_dtsi" | cut -d' ' -f1)" \
	"$expected_uboot_dtsi_sha256" "U-Boot DTSI hash"
expect_eq "$(sha256sum "$uboot_hook" | cut -d' ' -f1)" \
	"$expected_uboot_hook_sha256" "K11C Maxio hook hash"
expect_eq "$(sha256sum "$r14_dtb" | cut -d' ' -f1)" \
	"$expected_r14_dtb_sha256" "r14 handoff DTB hash"

work_dir=$(mktemp -d /home/user/work/k11c-r16.XXXXXX)
[[ "$work_dir" == /home/user/work/k11c-r16.* ]] || fail "unsafe work directory"
uboot_source_dts=$uboot_tree/dts/upstream/src/arm64/rockchip/rk3566-kickpi-k11c.dts
uboot_source_dtsi=$uboot_tree/arch/arm/dts/rk3566-kickpi-k11c-u-boot.dtsi
source_backup=$work_dir/rk3566-kickpi-k11c.source-backup.dts
source_installed=false

cleanup() {
	if [[ "$source_installed" == true && -s "$source_backup" ]]; then
		install -m 0644 "$source_backup" "$uboot_source_dts"
	fi
	if [[ -n "${work_dir:-}" && "$work_dir" == /home/user/work/k11c-r16.* ]]; then
		rm -rf -- "$work_dir"
	fi
}
trap cleanup EXIT

echo "[1/6] Compile the EEE-disabled Linux handoff DTB"
compile_linux_dtb "$variant_dts" "$work_dir/variant.dtb" "$work_dir/variant.pre.dts"
expect_absent "$work_dir/variant.dtb" /mmc@fe2c0000 vqmmc-supply
expect_eq "$("$host_bin/fdtget" "$work_dir/variant.dtb" /mmc@fe2c0000 status)" \
	okay "wireless SDIO status"
assert_wifi_pinctrl "$work_dir/variant.dtb"
assert_lan_dtb "$work_dir/variant.dtb"

echo "[2/6] Prove the r14-to-r16 DT delta is only Wi-Fi pinctrl plus EEE disable"
"$linux_tree/scripts/dtc/dtc" -I dtb -O dts -o "$work_dir/r14.dts" \
	"$r14_dtb" 2>/dev/null
"$linux_tree/scripts/dtc/dtc" -I dtb -O dts -o "$work_dir/r16.dts" \
	"$work_dir/variant.dtb" 2>/dev/null
diff -u "$work_dir/r14.dts" "$work_dir/r16.dts" > "$work_dir/r14-to-r16.diff" ||
	diff_status=$?
[[ "${diff_status:-0}" -eq 1 ]] || fail "r14-to-r16 DT comparison failed"
for expected_delta in \
	'^-[[:space:]]+rockchip,pins = <0x02 0x13 0x01 0xbb>;$' \
	'^\+[[:space:]]+rockchip,pins = <0x02 0x13 0x01 0xc0>;$' \
	'^-[[:space:]]+rockchip,pins = <0x02 0x12 0x01 0xbb>;$' \
	'^\+[[:space:]]+rockchip,pins = <0x02 0x12 0x01 0xc0>;$' \
	'^-[[:space:]]+pinctrl-0 = <0xc6>;$' \
	'^\+[[:space:]]+pinctrl-0 = <0xc6 0x7b 0x7c 0x7d 0x7e>;$' \
	'^\+[[:space:]]+eee-broken-100tx;$' \
	'^\+[[:space:]]+eee-broken-1000t;$' ; do
	[[ "$(grep -Ec "$expected_delta" "$work_dir/r14-to-r16.diff")" -eq 1 ]] ||
		fail "missing expected r14-to-r16 delta: $expected_delta"
done
if grep -E '^[+-]' "$work_dir/r14-to-r16.diff" | \
	grep -Ev '^(---|\+\+\+)|^[-+]([[:space:]]+rockchip,pins = <0x02 0x1[23] 0x01 0x(bb|c0)>;|[[:space:]]+pinctrl-0 = <0xc6( 0x7b 0x7c 0x7d 0x7e)?>;|[[:space:]]+eee-broken-(100tx|1000t);)$' \
	>/dev/null; then
	cat "$work_dir/r14-to-r16.diff" >&2
	fail "the Linux handoff DT changed beyond Wi-Fi pinctrl and EEE-disable properties"
fi

echo "[3/6] Build mainline U-Boot with the no-vqmmc handoff DT"
require_file "$uboot_source_dts"
require_file "$uboot_source_dtsi"
expect_eq "$(sha256sum "$uboot_source_dts" | cut -d' ' -f1)" \
	"$expected_uboot_source_dts_sha256" "pristine U-Boot source DTS"
expect_eq "$(sha256sum "$uboot_source_dtsi" | cut -d' ' -f1)" \
	"$expected_uboot_dtsi_sha256" "U-Boot source DTSI"
install -m 0644 "$uboot_source_dts" "$source_backup"
install -m 0644 "$variant_dts" "$uboot_source_dts"
source_installed=true

uboot_out=$work_dir/u-boot
make -C "$uboot_tree" O="$uboot_out" CROSS_COMPILE=aarch64-linux-gnu- \
	odroid-m1s-rk3566_defconfig > "$work_dir/u-boot-config.log" 2>&1
"$uboot_tree/scripts/config" --file "$uboot_out/.config" \
	--set-str DEFAULT_DEVICE_TREE "rockchip/rk3566-kickpi-k11c"
"$uboot_tree/scripts/config" --file "$uboot_out/.config" \
	--set-str OF_LIST "rockchip/rk3566-kickpi-k11c"
"$uboot_tree/scripts/config" --file "$uboot_out/.config" \
	--set-str SPL_OF_LIST "rockchip/rk3566-kickpi-k11c"
"$uboot_tree/scripts/config" --file "$uboot_out/.config" --disable ENV_IS_IN_MMC
"$uboot_tree/scripts/config" --file "$uboot_out/.config" --enable ENV_IS_NOWHERE
"$uboot_tree/scripts/config" --file "$uboot_out/.config" \
	--set-str BOOTCOMMAND 'fdt addr ${fdtcontroladdr}; fdt set /mmc@fe2c0000 status okay; bootflow scan -lbG mmc1; bootflow scan -lbG mmc0'
make -C "$uboot_tree" O="$uboot_out" CROSS_COMPILE=aarch64-linux-gnu- \
	olddefconfig >> "$work_dir/u-boot-config.log" 2>&1
if ! make -C "$uboot_tree" O="$uboot_out" CROSS_COMPILE=aarch64-linux-gnu- \
	BL31="$bl31_blob" ROCKCHIP_TPL="$ddr_blob" -j"$jobs" all \
	> "$work_dir/u-boot-build.log" 2>&1; then
	tail -100 "$work_dir/u-boot-build.log" >&2
	fail "U-Boot build failed"
fi

for file in "$uboot_out/u-boot.dtb" "$uboot_out/spl/u-boot-spl.dtb" \
	"$uboot_out/u-boot-rockchip.bin"; do
	require_file "$file"
done
expect_absent "$uboot_out/u-boot.dtb" /mmc@fe2c0000 vqmmc-supply
expect_eq "$("$host_bin/fdtget" "$uboot_out/u-boot.dtb" /mmc@fe2c0000 status)" \
	disabled "U-Boot wireless-probe suppression"
assert_wifi_pinctrl "$uboot_out/u-boot.dtb"
assert_lan_dtb "$uboot_out/u-boot.dtb"
grep -Fqx 'CONFIG_BOOTCOMMAND="fdt addr ${fdtcontroladdr}; fdt set /mmc@fe2c0000 status okay; bootflow scan -lbG mmc1; bootflow scan -lbG mmc0"' \
	"$uboot_out/.config" || fail "U-Boot boot command changed"

install -m 0644 "$source_backup" "$uboot_source_dts"
source_installed=false
expect_eq "$(sha256sum "$uboot_source_dts" | cut -d' ' -f1)" \
	"$expected_uboot_source_dts_sha256" "restored U-Boot source DTS"

echo "[4/6] Run focused negative invariant tests"
cp -- "$uboot_out/u-boot.dtb" "$work_dir/no-pcm.dtb"
"$host_bin/fdtput" -d "$work_dir/no-pcm.dtb" /sdio-pwrseq pinctrl-0
if (assert_wifi_pinctrl "$work_dir/no-pcm.dtb") >/dev/null 2>&1; then
	fail "Wi-Fi pinctrl negative mutation was accepted"
fi
cp -- "$uboot_out/u-boot.dtb" "$work_dir/wrong-lan-delay.dtb"
"$host_bin/fdtput" -t i "$work_dir/wrong-lan-delay.dtb" \
	/ethernet@fe010000 tx_delay 0
if (assert_lan_dtb "$work_dir/wrong-lan-delay.dtb") >/dev/null 2>&1; then
	fail "LAN negative mutation was accepted"
fi
for property in eee-broken-100tx eee-broken-1000t; do
	negative_dtb="$work_dir/missing-$property.dtb"
	cp -- "$uboot_out/u-boot.dtb" "$negative_dtb"
	"$host_bin/fdtput" -d "$negative_dtb" \
		/ethernet@fe010000/mdio/ethernet-phy@0 "$property"
	if (assert_lan_dtb "$negative_dtb") >/dev/null 2>&1; then
		fail "LAN $property negative mutation was accepted"
	fi
done

echo "[5/6] Inject only the new shim into the verified r3 HAOS image"
shim_length=$(stat -c %s "$uboot_out/u-boot-rockchip.bin")
first_partition_offset=$((first_partition_lba * sector_size))
(( shim_offset + shim_length <= first_partition_offset )) ||
	fail "U-Boot overlaps the first HAOS partition"
cp -- "$base_image" "$output_image"
dd if="$uboot_out/u-boot-rockchip.bin" of="$output_image" oflag=seek_bytes \
	seek="$shim_offset" bs=1M conv=notrunc status=none
cmp -s -n "$shim_offset" "$base_image" "$output_image" ||
	fail "bytes before the shim changed"
cmp -s -i "$((shim_offset + shim_length)):$((shim_offset + shim_length))" \
	"$base_image" "$output_image" || fail "bytes after the shim changed"
dd if="$output_image" of="$work_dir/readback.bin" iflag=skip_bytes,count_bytes \
	skip="$shim_offset" count="$shim_length" bs=1M status=none
cmp -s "$uboot_out/u-boot-rockchip.bin" "$work_dir/readback.bin" ||
	fail "U-Boot readback differs from the built binary"
sgdisk -v "$output_image" > "$work_dir/sgdisk.txt"

echo "[6/6] Verify every HAOS partition and publish artifacts"
declare -A expected_partition_hash=(
	[1]=443afa632e30dd209c50665d8e182ca7b72a10cb42f8b57b362236eaaccaef3f
	[2]=b17c58af130d9af41ad5071d57c6187a1e1156628b70f84679ec21ab9a298117
	[3]=76a978aa3e0e7641046397453bc663db7a7223ecd3a285343640df372679838e
	[4]=95aeaae03b56c171cf88753c821630a3c24f1fcf406cec3e17d56781aa3f8369
	[5]=a6d72ac7690f53be6ae46ba88506bd97302a093f7108472bd9efc3cefda06484
	[6]=2daeb1f36095b44b318410b3f4e8b5d989dcc7bb023d1426c492dab0a3053e74
	[7]=5dc4a4415d9c49e6275e0bc51f17da3666926cb7b0c720fb5bf256bb0318209c
	[8]=772a5174ded7b2d04bd55fe621747a5b70c9bb924da236b8b8b1499e55c47279
)
partition_records=()
for part in {1..8}; do
	start=$(sgdisk -i "$part" "$output_image" | awk '/First sector/ {print $3}')
	end=$(sgdisk -i "$part" "$output_image" | awk '/Last sector/ {print $3}')
	name=$(sgdisk -i "$part" "$output_image" | awk -F"'" '/Partition name/ {print $2}')
	[[ "$start" =~ ^[0-9]+$ ]] || fail "invalid start sector for partition $part"
	[[ "$end" =~ ^[0-9]+$ ]] || fail "invalid end sector for partition $part"
	[[ -n "$name" ]] || fail "missing name for partition $part"
	count=$((end - start + 1))
	hash=$(dd if="$output_image" bs=1M iflag=skip_bytes,count_bytes \
		skip="$((start * sector_size))" count="$((count * sector_size))" \
		status=none | sha256sum | cut -d' ' -f1)
	expect_eq "$hash" "${expected_partition_hash[$part]}" "partition $part hash"
	partition_records+=("partition.$name.sha256=$hash")
done
[[ "${#partition_records[@]}" -eq 8 ]] || fail "not all eight HAOS partitions were verified"

install -d -m 0755 "$record_dir" "$output_dir"
install -m 0644 "$uboot_out/u-boot-rockchip.bin" "$output_uboot"
install -m 0644 "$work_dir/variant.dtb" "$output_dtb"
install -m 0644 "$work_dir/u-boot-config.log" "$record_dir/u-boot-config.log"
install -m 0644 "$work_dir/u-boot-build.log" "$record_dir/u-boot-build.log"
install -m 0644 "$work_dir/sgdisk.txt" "$record_dir/sgdisk.txt"
install -m 0644 "$work_dir/r14-to-r16.diff" "$record_dir/r14-to-r16.diff"

# Boot only the R16 shim from microSD, then fall through to HAOS on eMMC.
# The empty partition area avoids duplicating the HAOS PARTUUIDs on the eMMC.
truncate -s 0 "$output_uboot_only_sd"
truncate -s "$first_partition_offset" "$output_uboot_only_sd"
dd if="$output_uboot" of="$output_uboot_only_sd" oflag=seek_bytes \
	seek="$shim_offset" bs=1M conv=notrunc status=none
cmp -s -n "$shim_offset" "$output_uboot_only_sd" /dev/zero ||
	fail "U-Boot-only SD prefix is not zero-filled"
cmp -s -n "$shim_length" -i "$shim_offset:0" \
	"$output_uboot_only_sd" "$output_uboot" ||
	fail "U-Boot-only SD embedded shim differs"
cmp -s -n "$((first_partition_offset - shim_offset - shim_length))" \
	-i "$((shim_offset + shim_length)):0" "$output_uboot_only_sd" /dev/zero ||
	fail "U-Boot-only SD suffix is not zero-filled"

output_sha256=$(sha256sum "$output_image" | cut -d' ' -f1)
uboot_sha256=$(sha256sum "$output_uboot" | cut -d' ' -f1)
dtb_sha256=$(sha256sum "$output_dtb" | cut -d' ' -f1)
uboot_only_sd_sha256=$(sha256sum "$output_uboot_only_sd" | cut -d' ' -f1)
{
	printf 'base_artifact=%s\n' "$(basename "$base_image")"
	printf 'base_artifact_sha256=%s\n' "$expected_base_sha256"
	printf 'output_sha256=%s\n' "$output_sha256"
	printf 'uboot_artifact=%s\n' "$(basename "$output_uboot")"
	printf 'uboot_sha256=%s\n' "$uboot_sha256"
	printf 'linux_handoff_dtb_artifact=%s\n' "$(basename "$output_dtb")"
	printf 'linux_handoff_dtb_sha256=%s\n' "$dtb_sha256"
	printf 'uboot_only_sd_artifact=%s\n' "$(basename "$output_uboot_only_sd")"
	printf 'uboot_only_sd_sha256=%s\n' "$uboot_only_sd_sha256"
	printf 'uboot_hook_sha256=%s\n' "$expected_uboot_hook_sha256"
	printf 'r14_to_r16_dtb_delta=sdio-pwrseq-pcm-pinctrl-plus-eee-disable\n'
	printf 'eee_broken_100tx=true\n'
	printf 'eee_broken_1000t=true\n'
	printf 'only_raw_uboot_shim_changed_from_r3=true\n'
	printf 'all_generic_haos_partitions_preserved=true\n'
	printf 'i2s2_controller_enabled=false\n'
	printf 'sdmmc1_vqmmc_supply_present=false\n'
	printf 'boot_order=microSD-then-eMMC\n'
	printf 'gpt_validation=pass\n'
	printf '%s\n' "${partition_records[@]}"
	printf 'purpose=K11C persistent EEE-disable LAN compatibility test\n'
	printf 'ota_release_image=false\n'
} > "$manifest"

echo "PASS: created $output_image"
echo "SHA256: $output_sha256"
echo "U-Boot: $output_uboot"
echo "U-Boot SHA256: $uboot_sha256"
echo "Linux handoff DTB: $output_dtb"
echo "Linux DTB SHA256: $dtb_sha256"
echo "U-Boot-only SD image: $output_uboot_only_sd"
echo "U-Boot-only SD SHA256: $uboot_only_sd_sha256"
echo "Manifest: $manifest"
