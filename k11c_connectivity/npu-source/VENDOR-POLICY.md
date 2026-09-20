# K11C r22 manufacturer policy port

Reference: manufacturer SDK `rk356x-linux-2026051515.tar.gz`, Git commit
`22a87c6c96feef811d103e04085b46b7d7ec4786`, Linux 5.10.160. Selected unmodified
files and their hashes are in `vendor-reference/`. The captured working board
used Linux 5.10.157; the downloaded SDK is not asserted to be its exact build.

The captured live DT SHA256 is
`80785326d51559994f4f44a564953adfcc03a260ef3c88307fd318775f7e5bb1`.
Its entire NPU OPP subtree is used, with phandles remapped to the mainline tree.
Unrelated LAN, wireless pinctrl, audio, GPU and storage nodes remain unchanged.

| Manufacturer path | Mainline adaptation |
| --- | --- |
| RK3568 OTP ECC read | `k11c_rk3568_otp.c`, read-only NVMEM provider and original read sequence |
| RK356x serial/remark bin and PVTM L0-L3 | `k11c_vendor_configure`, public supported-hw/property APIs |
| OTP voltage offset, MBIST floor, voltage ceiling | `k11c_vendor_prepare` and `include/k11c_vendor_opp.h` |
| +50 mV cold correction, 0/5 C hysteresis, 200 ms monitor | `k11c_vendor_opp.c`, public thermal/OPP/QoS APIs |
| RK809 regulator-init-microvolt=900000 | Applied before NPU power-on; mainline regulator core ignores this BSP property |
| BSP initial SCMI 600 MHz, suspended 200 MHz, resume saved request | 0.5.1 requests the highest enabled OPP up to 900 MHz; idle/resume callbacks retained |
| rknpu_ondemand stored frequency request | Retained; no utilization-based frequency policy added |

The boot 900 mV value is an initialization value, not a fixed operating voltage.
The operating voltage comes from the selected table and OTP/temperature correction.
The 1 GHz OPP nodes remain disabled. No high-temperature NPU table is invented:
the captured board DT does not configure one. Existing CPU thermal trips remain.

## Compatibility differences

- The HAOS kernel and Frigate App are unchanged. External modules are compiled
  specifically for `6.18.39-haos`. No private OPP structure layout is assumed.
- The BSP system-wide monitor is adapted as an NPU-only worker using the same
  TSADC channel. Mainline calls this zone `cpu-thermal`, BSP `soc-thermal`.
- Blank PVTM requires the BSP's CPU-populated live-PVTM measurement/cache path,
  which is not ported here. Blank/unreadable cells or invalid MBIST index stop
  NPU activation instead of continuing with defaults. No OTP is programmed.
- Voltage/clock errors are propagated and verified by readback. The App checks
  a locked status snapshot and refuses old modules rather than force-unloading
  them. Version 0.5.1 uses a normal request up to 900 MHz. The request survives
  a cold-limited initialization so that the existing QoS limit can release it
  when warm. This default differs from the BSP's 600 MHz initial request;
  changing devfreq policy from another program is outside this App profile.
- The previously working non-IOMMU 128 MiB CMA backend is preserved. This is a
  voltage/initialization-policy port, not replacement of the entire BSP driver.

## Verification scope

The local checks compare voltage/bin/cold-policy calculations with the original
manufacturer C functions, execute the production OTP and clock/voltage functions
with simulated hardware, and exercise the shipped App paths with fake sysfs.
Negative and mutation tests reject missing guards and incorrect sequences.
The complete built DT is compared against the previous working DT plus only
the approved vendor-policy changes. These checks do not replace board testing.

Original Rockchip code and adaptations retain GPL-2.0 licensing. App control and
packaging code remain AGPL-3.0-or-later as marked per file.
