/* SPDX-License-Identifier: GPL-2.0-only */
/* RK3566 single-supply subset of Rockchip OPP/system-monitor policy. */
#ifndef K11C_VENDOR_OPP_H
#define K11C_VENDOR_OPP_H

#define K11C_OPP_MAX 12
struct k11c_voltage {
	unsigned long hz, uv, min_uv, max_uv, cold_uv;
};
struct k11c_otp_adjust {
	unsigned int min_mhz, max_mhz, add_mv, mbist_uv;
};

/* Values are DT cells or already bit-extracted NVMEM cells, never guesses. */
static inline unsigned int k11c_vendor_bin(unsigned int spec, unsigned int remark)
{
	unsigned int value = remark ? remark : spec;
	if (value == 0x0d)
		return 1;
	if (value == 0x0a)
		return 2;
	return 0;
}

static inline int k11c_vendor_sel(unsigned int pvtm, const unsigned int *table,
				 unsigned int rows)
{
	int selected = -1;
	unsigned int i;
	/* BSP rockchip_get_sel: lower bounds are used, including the last row. */
	for (i = 0; i < rows; i++)
		if (pvtm >= table[3 * i])
			selected = table[3 * i + 2];
	return selected;
}

static inline unsigned long k11c_vendor_adjust(struct k11c_voltage *opps,
		unsigned int count, const struct k11c_otp_adjust *otp,
		unsigned long max_uv)
{
	unsigned int i;
	unsigned long last_safe = 0, safe = 0;
	int reached_max = 0;
	for (i = 0; i < count; i++) {
		struct k11c_voltage *o = &opps[i];
		/* BSP order: OTP delta, MBIST minimum, then IR-drop/max clamp. */
		if (otp->add_mv && o->hz >= otp->min_mhz * 1000000UL &&
		    o->hz <= otp->max_mhz * 1000000UL) {
			o->uv += otp->add_mv * 1000UL;
			if (o->uv > o->max_uv)
				o->uv = o->max_uv;
		}
		if (o->uv < otp->mbist_uv)
			o->uv = o->min_uv = otp->mbist_uv;
		/* No board-irdrop table is present on the captured K11C DT. */
		if (o->uv <= max_uv) {
			if (o->max_uv > max_uv)
				o->max_uv = max_uv;
			if (!reached_max)
				last_safe = o->hz;
			if (o->uv == max_uv)
				reached_max = 1;
		} else {
			o->uv = o->min_uv = o->max_uv = max_uv;
		}
		if (last_safe != o->hz)
			safe = last_safe;
	}
	return safe;
}

static inline unsigned long k11c_vendor_cold_table(struct k11c_voltage *opps,
		unsigned int count, unsigned long max_uv, unsigned int delta_uv)
{
	unsigned int i;
	unsigned long last_safe = 0, limit = 0;
	int reached_max = 0;
	for (i = 0; i < count; i++) {
		struct k11c_voltage *o = &opps[i];
		if (o->uv + delta_uv <= max_uv) {
			o->cold_uv = o->uv + delta_uv;
			if (!reached_max)
				last_safe = o->hz;
			if (o->cold_uv == max_uv)
				reached_max = 1;
		} else {
			o->cold_uv = max_uv;
		}
		if (last_safe && last_safe != o->hz)
			limit = last_safe;
	}
	return limit;
}

static inline int k11c_vendor_cold_state(int previous, int temp,
				       int low, int hysteresis)
{
	if (temp < low)
		return 1;
	if (temp > low + hysteresis)
		return 0;
	return previous;
}

#ifdef __KERNEL__
struct rknpu_device;
struct k11c_vendor_state;
int k11c_vendor_boot_voltage(struct rknpu_device *npu);
int k11c_vendor_configure(struct rknpu_device *npu);
int k11c_vendor_prepare(struct rknpu_device *npu);
int k11c_vendor_set_rate(struct rknpu_device *npu, unsigned long hz);
int k11c_vendor_monitor_start(struct rknpu_device *npu);
void k11c_vendor_monitor_stop(struct rknpu_device *npu);
int k11c_vendor_suspend(struct rknpu_device *npu);
int k11c_vendor_resume(struct rknpu_device *npu);
#endif
#endif
