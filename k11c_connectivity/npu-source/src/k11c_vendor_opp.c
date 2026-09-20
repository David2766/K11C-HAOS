// SPDX-License-Identifier: GPL-2.0-only
/*
 * K11C RK3566 BSP OPP policy on public Linux 6.18 APIs.
 * Derived from Rockchip rockchip_opp_select.c / rockchip_system_monitor.c.
 * Copyright (C) Rockchip Electronics Co., Ltd.
 * See diagnostics/npu-r22 for source pins and differential tests.
 *
 * Only the paths selected by the captured K11C DT are ported: one supply,
 * SCMI clock, OTP PVTM, bin masks, OTP voltage delta, MBIST and cold voltage.
 * Unknown policy properties are not silently treated as supported.
 */
#include <linux/clk.h>
#include <linux/devfreq.h>
#include <linux/nvmem-consumer.h>
#include <linux/of.h>
#include <linux/pm_opp.h>
#include <linux/pm_qos.h>
#include <linux/pm_runtime.h>
#include <linux/regulator/consumer.h>
#include <linux/slab.h>
#include <linux/thermal.h>
#include <linux/unaligned.h>
#include <linux/workqueue.h>
#include "rknpu_drv.h"
#include "k11c_vendor_opp.h"

struct k11c_vendor_state {
	struct rknpu_device *npu;
	struct k11c_voltage opps[K11C_OPP_MAX];
	struct k11c_otp_adjust otp;
	unsigned int count, bin, selector, pvtm;
	unsigned int max_uv, low, hysteresis, cold_delta;
	unsigned long cold_limit;
	struct thermal_zone_device *tz;
	struct delayed_work work;
	struct dev_pm_qos_request max_req;
	bool cold, started;
	int error;
};

int k11c_vendor_boot_voltage(struct rknpu_device *npu)
{
	struct device_node *reg;
	u32 uv, policy;
	int ret;
	if (!of_find_property(npu->dev->of_node, "k11c,vendor-policy", NULL))
		return 0;
	if (of_property_read_u32(npu->dev->of_node, "k11c,vendor-policy", &policy) ||
	    policy != 1 || !of_machine_is_compatible("kickpi,k11c") ||
	    !of_machine_is_compatible("rockchip,rk3566"))
		return -EINVAL;
	if (!npu->vdd)
		return -ENODEV;
	reg = of_parse_phandle(npu->dev->of_node, "rknpu-supply", 0);
	if (!reg)
		return -EINVAL;
	ret = of_property_read_u32(reg, "regulator-init-microvolt", &uv);
	of_node_put(reg);
	if (ret || uv != 900000)
		return -EINVAL;
	/* Vendor regulator core does this before NPU probe. Mainline ignores
	 * regulator-init-microvolt, so apply it before enabling the NPU clocks. */
	ret = regulator_set_voltage(npu->vdd, uv, uv);
	if (ret)
		return ret;
	ret = regulator_get_voltage(npu->vdd);
	return ret == uv ? 0 : (ret < 0 ? ret : -ERANGE);
}

static int read_cell(struct device_node *np, const char *name, void *out, size_t size)
{
	struct nvmem_cell *cell;
	void *buf;
	size_t len;
	int ret = 0;
	cell = of_nvmem_cell_get(np, name);
	if (IS_ERR(cell))
		return PTR_ERR(cell);
	buf = nvmem_cell_read(cell, &len);
	if (IS_ERR(buf)) {
		ret = PTR_ERR(buf);
	} else {
		if (len != size)
			ret = -EINVAL;
		else
			memcpy(out, buf, size);
		kfree(buf);
	}
	nvmem_cell_put(cell);
	return ret;
}

int k11c_vendor_configure(struct rknpu_device *npu)
{
	struct device *dev = npu->dev;
	struct device_node *np, *thermal;
	struct k11c_vendor_state *v;
	unsigned int table[12], hw[2], cold[3], policy;
	struct dev_pm_opp_config config = {};
	u8 spec, remark, mbist, info[6], pvtm[2];
	char prop[8];
	int ret;

	if (!of_find_property(dev->of_node, "k11c,vendor-policy", NULL))
		return 0;
	if (of_property_read_u32(dev->of_node, "k11c,vendor-policy", &policy) || policy != 1 ||
	    !of_machine_is_compatible("kickpi,k11c") ||
	    !of_machine_is_compatible("rockchip,rk3566"))
		return -EINVAL;
	v = devm_kzalloc(dev, sizeof(*v), GFP_KERNEL);
	if (!v)
		return -ENOMEM;
	v->npu = npu;
	np = of_parse_phandle(dev->of_node, "operating-points-v2", 0);
	if (!np)
		return -EINVAL;
	ret = -EINVAL;
	/* No AVS/IR-drop/private PLL policy is configured in this board's DT. */
	if (of_find_property(np, "rockchip,board-irdrop", NULL) ||
	    of_find_property(np, "rockchip,avs", NULL) ||
	    of_find_property(np, "rockchip,avs-enable", NULL) ||
	    of_find_property(np, "rockchip,temp-freq-table", NULL) ||
	    of_find_property(np, "rockchip,high-temp", NULL) ||
	    !of_property_read_bool(np, "rockchip,supported-hw"))
		goto out;
	if (of_property_read_u32(np, "rockchip,max-volt", &v->max_uv) ||
	    of_property_read_u32(np, "rockchip,low-temp", &v->low) ||
	    of_property_read_u32(np, "rockchip,temp-hysteresis", &v->hysteresis) ||
	    of_property_count_u32_elems(np, "rockchip,low-temp-adjust-volt") != 3 ||
	    of_property_read_u32_array(np, "rockchip,low-temp-adjust-volt", cold, 3) ||
	    of_property_count_u32_elems(np, "rockchip,pvtm-voltage-sel") != 12 ||
	    of_property_read_u32_array(np, "rockchip,pvtm-voltage-sel", table, 12))
		goto out;
	if (cold[0] != 0 || cold[1] != 1000 || cold[2] != 50000 ||
	    v->max_uv != 1000000 || v->low != 0 || v->hysteresis != 5000)
		goto out;
	v->cold_delta = cold[2];
	/* Read errors are not zero-valued fuses. Never guess a bin on failure. */
	ret = read_cell(np, "specification_serial_number", &spec, 1);
	if (!ret)
		ret = read_cell(np, "remark_spec_serial_number", &remark, 1);
	if (!ret)
		ret = read_cell(np, "pvtm", pvtm, sizeof(pvtm));
	if (!ret)
		ret = read_cell(np, "mbist-vmin", &mbist, 1);
	if (!ret)
		ret = read_cell(np, "opp-info", info, sizeof(info));
	if (ret)
		goto out;
	v->bin = k11c_vendor_bin(spec, remark);
	v->pvtm = get_unaligned_le16(pvtm) * 10U;
	/* The BSP's CPU-populated live-PVTM cache is absent in mainline HAOS. */
	if (!v->pvtm) {
		dev_err(dev, "OTP PVTM is blank; BSP live-PVTM fallback is required, refusing guessed voltage\n");
		ret = -ENODATA;
		goto out;
	}
	ret = k11c_vendor_sel(v->pvtm, table, 4);
	if (ret < 0 || ret > 3) {
		ret = -EINVAL;
		goto out;
	}
	v->selector = ret;
	v->otp.min_mhz = get_unaligned_le16(info);
	v->otp.max_mhz = get_unaligned_le16(info + 2);
	v->otp.add_mv = info[4];
	if (mbist && of_property_read_u32_index(np, "mbist-vmin", mbist - 1,
					       &v->otp.mbist_uv)) {
		ret = -EINVAL;
		goto out;
	}
	hw[0] = BIT(v->bin);
	hw[1] = BIT(v->selector);
	snprintf(prop, sizeof(prop), "L%u", v->selector);
	config.supported_hw = hw;
	config.supported_hw_count = ARRAY_SIZE(hw);
	config.prop_name = prop;
	ret = devm_pm_opp_set_config(dev, &config);
	if (ret)
		goto out;
	thermal = of_parse_phandle(dev->of_node, "k11c,thermal-zone", 0);
	if (!thermal) {
		ret = -EINVAL;
		goto out;
	}
	/* Same TSADC channel 0; mainline names it cpu-thermal, BSP soc-thermal. */
	v->tz = thermal_zone_get_zone_by_name(thermal->name);
	of_node_put(thermal);
	if (IS_ERR(v->tz)) {
		ret = PTR_ERR(v->tz);
		goto out;
	}
	npu->vendor = v;
	dev_info(dev, "K11C_VENDOR_SELECT bin=%u pvtm=%u L%u mbist_uv=%u otp=%u..%uMHz+%umV\n",
		 v->bin, v->pvtm, v->selector, v->otp.mbist_uv,
		 v->otp.min_mhz, v->otp.max_mhz, v->otp.add_mv);
	ret = 0;
out:
	of_node_put(np);
	return ret;
}

static int set_table_voltage(struct k11c_vendor_state *v, bool cold)
{
	unsigned int i;
	int ret;
	for (i = 0; i < v->count; i++) {
		struct k11c_voltage *o = &v->opps[i];
		unsigned long uv = cold ? o->cold_uv : o->uv;
		/* BSP restore sets minimum to the adjusted normal target too. */
		ret = dev_pm_opp_adjust_voltage(v->npu->dev, o->hz, uv, uv, o->max_uv);
		if (ret)
			return ret;
	}
	return 0;
}

int k11c_vendor_prepare(struct rknpu_device *npu)
{
	struct k11c_vendor_state *v = npu->vendor;
	struct dev_pm_opp *opp;
	struct dev_pm_opp_supply supply;
	unsigned long hz = 0, safe;
	unsigned int i, kept = 0;
	int ret, temp;
	if (!v)
		return 0;
	while (true) {
		opp = dev_pm_opp_find_freq_ceil(npu->dev, &hz);
		if (IS_ERR(opp)) {
			if (PTR_ERR(opp) == -ERANGE)
				break;
			return PTR_ERR(opp);
		}
		ret = dev_pm_opp_get_supplies(opp, &supply);
		dev_pm_opp_put(opp);
		if (ret)
			return ret;
		if (v->count == K11C_OPP_MAX || !supply.u_volt ||
		    supply.u_volt < supply.u_volt_min || supply.u_volt > supply.u_volt_max)
			return -EINVAL;
		v->opps[v->count++] = (struct k11c_voltage){
			.hz = hz, .uv = supply.u_volt,
			.min_uv = supply.u_volt_min, .max_uv = supply.u_volt_max,
		};
		hz++;
	}
	if (!v->count)
		return -ENODATA;
	safe = k11c_vendor_adjust(v->opps, v->count, &v->otp, v->max_uv);
	for (i = 0; i < v->count; i++) {
		struct k11c_voltage *o = &v->opps[i];
		if (safe && o->hz > safe) {
			ret = dev_pm_opp_disable(npu->dev, o->hz);
		} else {
			ret = dev_pm_opp_adjust_voltage(npu->dev, o->hz, o->uv, o->min_uv, o->max_uv);
			v->opps[kept++] = *o;
		}
		if (ret)
			return ret;
	}
	v->count = kept;
	v->cold_limit = k11c_vendor_cold_table(v->opps, v->count, v->max_uv, v->cold_delta);
	/* BSP initial state: cold unless a valid sample is greater than 5 C. */
	v->cold = true;
	ret = thermal_zone_get_temp(v->tz, &temp);
	if (!ret && temp != THERMAL_TEMP_INVALID)
		v->cold = k11c_vendor_cold_state(true, temp, v->low, v->hysteresis);
	return set_table_voltage(v, v->cold);
}

/* Called with devfreq_lock held after initialization. One regulator vote is
 * used for both frequency transitions and same-frequency thermal adjustment;
 * this avoids the mainline OPP core's same-OPP early return. RK3566 has no
 * BSP set_read_margin callback and no second/memory supply. */
int k11c_vendor_set_rate(struct rknpu_device *npu, unsigned long hz)
{
	struct k11c_vendor_state *v = npu->vendor;
	struct dev_pm_opp *opp;
	struct dev_pm_opp_supply s;
	unsigned long old_hz, actual, clock_hz = hz;
	int ret, uv;
	if (v->cold && v->cold_limit && hz > v->cold_limit)
		hz = v->cold_limit;
	opp = dev_pm_opp_find_freq_ceil(npu->dev, &hz);
	if (IS_ERR(opp))
		return PTR_ERR(opp);
	ret = dev_pm_opp_get_supplies(opp, &s);
	dev_pm_opp_put(opp);
	if (ret)
		return ret;
	clock_hz = hz;
	old_hz = clk_get_rate(npu->clks[0].clk);
	/* Down: frequency before voltage. Up/equal: voltage before frequency. */
	if (clock_hz < old_hz) {
		ret = clk_set_rate(npu->clks[0].clk, clock_hz);
		if (ret)
			return ret;
	}
	/* BSP monitor restores min=adjusted target; public adjust_voltage can
	 * skip an unchanged target's min update, so enforce it on the vote. */
	ret = regulator_set_voltage_triplet(npu->vdd, s.u_volt, s.u_volt, s.u_volt_max);
	if (ret)
		return ret;
	uv = regulator_get_voltage(npu->vdd);
	if (uv < 0)
		return uv;
	if (uv < s.u_volt || uv > s.u_volt_max)
		return -ERANGE;
	if (clock_hz >= old_hz) {
		ret = clk_set_rate(npu->clks[0].clk, clock_hz);
		if (ret)
			return ret;
	}
	actual = clk_get_rate(npu->clks[0].clk);
	if (!actual || actual > clock_hz)
		return -ERANGE;
	npu->current_freq = hz;
	npu->current_volt = uv;
	return 0;
}

static void monitor_work(struct work_struct *work)
{
	struct k11c_vendor_state *v = container_of(to_delayed_work(work), struct k11c_vendor_state, work);
	struct rknpu_device *npu = v->npu;
	int ret, temp;
	bool cold;
	ret = thermal_zone_get_temp(v->tz, &temp);
	if (!ret && temp != THERMAL_TEMP_INVALID) {
		cold = k11c_vendor_cold_state(v->cold, temp, v->low, v->hysteresis);
		if (cold != v->cold) {
			mutex_lock(&npu->devfreq_lock);
			ret = set_table_voltage(v, cold);
			if (!ret)
				v->cold = cold;
			mutex_unlock(&npu->devfreq_lock);
			if (!ret && v->cold_limit)
				ret = dev_pm_qos_update_request(&v->max_req,
					cold ? v->cold_limit / 1000 : PM_QOS_MAX_FREQUENCY_DEFAULT_VALUE);
			if (ret >= 0) {
				unsigned long requested;
				mutex_lock(&npu->devfreq_lock);
				/* BSP check_rate_volt checks the live (possibly idle) clock
				 * without replacing the saved runtime-resume request. */
				requested = npu->current_freq;
				ret = k11c_vendor_set_rate(npu, clk_get_rate(npu->clks[0].clk));
				npu->current_freq = requested;
				mutex_unlock(&npu->devfreq_lock);
			}
			if (ret < 0) {
				v->error = ret;
				dev_err(npu->dev, "vendor thermal voltage adjustment failed: %d\n", ret);
				return;
			}
		}
	}
	if (READ_ONCE(v->started))
		schedule_delayed_work(&v->work, msecs_to_jiffies(200));
}

static ssize_t vendor_policy_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	struct rknpu_device *npu = dev_get_drvdata(dev);
	struct k11c_vendor_state *v = npu->vendor;
	unsigned long hz;
	struct dev_pm_opp *opp;
	struct dev_pm_opp_supply s;
	int ret, uv;
	if (!v)
		return -ENODEV;
	mutex_lock(&npu->devfreq_lock);
	hz = npu->current_freq;
	opp = dev_pm_opp_find_freq_ceil(dev, &hz);
	if (IS_ERR(opp)) {
		ret = PTR_ERR(opp);
		goto out;
	}
	ret = dev_pm_opp_get_supplies(opp, &s);
	dev_pm_opp_put(opp);
	if (ret)
		goto out;
	uv = regulator_get_voltage(npu->vdd);
	ret = sysfs_emit(buf, "revision=1 bin=%u pvtm=%u selector=%u cold=%u target_hz=%lu clock_hz=%lu voltage_uv=%d min_uv=%lu max_uv=%lu error=%d\n",
		v->bin, v->pvtm, v->selector, v->cold, hz, clk_get_rate(npu->clks[0].clk),
		uv, s.u_volt, s.u_volt_max, v->error);
out:
	mutex_unlock(&npu->devfreq_lock);
	return ret;
}
static DEVICE_ATTR_RO(vendor_policy);
static struct attribute *vendor_attrs[] = { &dev_attr_vendor_policy.attr, NULL };
static const struct attribute_group vendor_group = { .attrs = vendor_attrs };

void k11c_vendor_monitor_stop(struct rknpu_device *npu)
{
	struct k11c_vendor_state *v = npu->vendor;
	if (!v || !v->started)
		return;
	WRITE_ONCE(v->started, false);
	cancel_delayed_work_sync(&v->work);
	if (dev_pm_qos_request_active(&v->max_req))
		dev_pm_qos_remove_request(&v->max_req);
}

static void stop_action(void *data)
{
	k11c_vendor_monitor_stop(data);
}

int k11c_vendor_monitor_start(struct rknpu_device *npu)
{
	struct k11c_vendor_state *v = npu->vendor;
	int ret;
	if (!v)
		return 0;
	INIT_DELAYED_WORK(&v->work, monitor_work);
	ret = dev_pm_qos_add_request(npu->dev, &v->max_req, DEV_PM_QOS_MAX_FREQUENCY,
		v->cold && v->cold_limit ? v->cold_limit / 1000 : PM_QOS_MAX_FREQUENCY_DEFAULT_VALUE);
	if (ret < 0)
		return ret;
	v->started = true;
	ret = devm_add_action_or_reset(npu->dev, stop_action, npu);
	if (ret)
		return ret;
	ret = devm_device_add_group(npu->dev, &vendor_group);
	if (ret)
		return ret;
	schedule_delayed_work(&v->work, msecs_to_jiffies(200));
	return 0;
}

int k11c_vendor_suspend(struct rknpu_device *npu)
{
	if (!npu->vendor)
		return 0;
	return clk_set_rate(npu->clks[0].clk, 200000000);
}

int k11c_vendor_resume(struct rknpu_device *npu)
{
	if (!npu->vendor || !npu->current_freq || !npu->current_volt)
		return 0;
	return clk_set_rate(npu->clks[0].clk, npu->current_freq);
}
