// SPDX-License-Identifier: GPL-2.0
/*
 * Copyright (C) Rockchip Electronics Co., Ltd.
 * Author: Finley Xiao <finley.xiao@rock-chips.com>
 *
 * Mainline API adapter. K11C vendor-policy DTs use the RK3566 BSP
 * selection/thermal/runtime path in k11c_vendor_opp.c. Earlier single-OPP
 * diagnostic DTs retain their original initialization behavior.
 */

#include <linux/clk.h>
#include <linux/devfreq.h>
#include <linux/version.h>
#if LINUX_VERSION_CODE >= KERNEL_VERSION(6, 19, 0)
#include <linux/devfreq-governor.h>
#else
#include "governor.h"
#endif
#include <linux/pm_opp.h>
#include <linux/pm_runtime.h>
#include <linux/regulator/consumer.h>

#include "rknpu_drv.h"
#include "rknpu_devfreq.h"
#include "k11c_vendor_opp.h"

/* ------------------------------------------------------------------ */
/*  Custom "rknpu_ondemand" governor                                  */
/*                                                                    */
/*  The BSP governor forwards the stored request. It does NOT ramp   */
/*  frequency on each submission or implement a utilization loop.    */
/* ------------------------------------------------------------------ */

static int devfreq_rknpu_ondemand_func(struct devfreq *df,
				       unsigned long *freq)
{
	struct rknpu_device *rknpu_dev = df->data;

	if (rknpu_dev && rknpu_dev->ondemand_freq)
		*freq = rknpu_dev->ondemand_freq;
	else
		*freq = df->previous_freq;

	return 0;
}

static int devfreq_rknpu_ondemand_handler(struct devfreq *devfreq,
					  unsigned int event, void *data)
{
	return 0;
}

static struct devfreq_governor devfreq_rknpu_ondemand = {
	.name = "rknpu_ondemand",
	.get_target_freq = devfreq_rknpu_ondemand_func,
	.event_handler = devfreq_rknpu_ondemand_handler,
};

/* ------------------------------------------------------------------ */
/*  devfreq profile callbacks                                         */
/* ------------------------------------------------------------------ */

static int npu_devfreq_target(struct device *dev, unsigned long *freq,
			      u32 flags)
{
	struct rknpu_device *rknpu_dev = dev_get_drvdata(dev);
	struct dev_pm_opp *opp;
	unsigned long opp_volt;
	int ret;

	if (rknpu_dev->vendor) {
		opp = devfreq_recommended_opp(dev, freq, flags);
		if (IS_ERR(opp))
			return PTR_ERR(opp);
		dev_pm_opp_put(opp);
		mutex_lock(&rknpu_dev->devfreq_lock);
		ret = k11c_vendor_set_rate(rknpu_dev, *freq);
		if (!ret) {
			*freq = rknpu_dev->current_freq;
			if (rknpu_dev->devfreq)
				rknpu_dev->devfreq->last_status.current_frequency = *freq;
		}
		mutex_unlock(&rknpu_dev->devfreq_lock);
		return ret;
	}

	opp = devfreq_recommended_opp(dev, freq, flags);
	if (IS_ERR(opp))
		return PTR_ERR(opp);
	opp_volt = dev_pm_opp_get_voltage(opp);
	dev_pm_opp_put(opp);

	if (*freq == rknpu_dev->current_freq)
		return 0;

	mutex_lock(&rknpu_dev->devfreq_lock);
	ret = dev_pm_opp_set_rate(dev, *freq);
	if (!ret) {
		rknpu_dev->current_freq = *freq;
		if (rknpu_dev->devfreq)
			rknpu_dev->devfreq->last_status.current_frequency =
				*freq;
		rknpu_dev->current_volt = opp_volt;
		LOG_DEV_DEBUG(dev, "set rknpu freq: %lu, volt: %lu\n",
			      rknpu_dev->current_freq,
			      rknpu_dev->current_volt);
	}
	mutex_unlock(&rknpu_dev->devfreq_lock);

	return ret;
}

static int npu_devfreq_get_dev_status(struct device *dev,
				      struct devfreq_dev_status *stat)
{
	struct rknpu_device *rknpu_dev = dev_get_drvdata(dev);
	struct rknpu_subcore_data *subcore_data;
	unsigned long flags;
	ktime_t busy = 0;

	/* Manufacturer RK3566 get_dev_status is a no-op. */
	if (rknpu_dev->vendor)
		return 0;

	stat->current_frequency = rknpu_dev->current_freq;

	/*
	 * Aggregate busy_time across all subcores.  The hrtimer handler
	 * snapshots total_busy_time every RKNPU_LOAD_INTERVAL (1 s).
	 */
	for (int i = 0; i < rknpu_dev->config->num_irqs; i++) {
		subcore_data = &rknpu_dev->subcore_datas[i];
		spin_lock_irqsave(&rknpu_dev->irq_lock, flags);
		busy += subcore_data->timer.total_busy_time;
		spin_unlock_irqrestore(&rknpu_dev->irq_lock, flags);
	}

	/* Express times in microseconds for the devfreq governor. */
	stat->busy_time = ktime_to_us(busy);
	stat->total_time = RKNPU_LOAD_INTERVAL / 1000; /* ns -> us */

	return 0;
}

static int npu_devfreq_get_cur_freq(struct device *dev, unsigned long *freq)
{
	struct rknpu_device *rknpu_dev = dev_get_drvdata(dev);

	*freq = rknpu_dev->current_freq;
	return 0;
}

static struct devfreq_dev_profile npu_devfreq_profile = {
	.polling_ms = 50,
	.target = npu_devfreq_target,
	.get_dev_status = npu_devfreq_get_dev_status,
	.get_cur_freq = npu_devfreq_get_cur_freq,
};

/* ------------------------------------------------------------------ */
/*  Public API consumed by rknpu_drv.c                                */
/* ------------------------------------------------------------------ */

void rknpu_devfreq_lock(struct rknpu_device *rknpu_dev)
{
	if (rknpu_dev->devfreq)
		mutex_lock(&rknpu_dev->devfreq_lock);
}
EXPORT_SYMBOL(rknpu_devfreq_lock);

void rknpu_devfreq_unlock(struct rknpu_device *rknpu_dev)
{
	if (rknpu_dev->devfreq)
		mutex_unlock(&rknpu_dev->devfreq_lock);
}
EXPORT_SYMBOL(rknpu_devfreq_unlock);

/* Release the enable vote taken by dev_pm_opp_set_rate() on probe unwind. */
static void rknpu_initial_opp_disable(void *data)
{
	struct device *dev = data;
	int ret = dev_pm_opp_set_rate(dev, 0);

	if (ret)
		LOG_DEV_ERROR(dev, "failed to release initial OPP: %d\n", ret);
}

int rknpu_devfreq_init(struct rknpu_device *rknpu_dev)
{
	struct device *dev = rknpu_dev->dev;
	struct devfreq_dev_profile *dp = &npu_devfreq_profile;
	struct dev_pm_opp *opp;
	struct dev_pm_opp_supply supply;
	unsigned long initial_freq, requested_freq;
	int initial_uv;
	int ret;
	static const char * const reg_names[] = { "rknpu", NULL };

	/*
	 * Tell the OPP framework which regulator and clock to manage.
	 * "rknpu" matches the DT property "rknpu-supply = <&vdd_npu>".
	 * "scmi_clk" is the first clock in the NPU node's clocks list
	 * and the one whose rate actually controls the NPU frequency
	 * through the SCMI firmware.
	 */
	ret = devm_pm_opp_set_regulators(dev, reg_names);
	if (ret) {
		LOG_DEV_ERROR(dev, "failed to set OPP regulators: %d\n", ret);
		return ret;
	}

	ret = devm_pm_opp_set_clkname(dev, "scmi_clk");
	if (ret) {
		LOG_DEV_ERROR(dev, "failed to set OPP clkname: %d\n", ret);
		return ret;
	}

	ret = k11c_vendor_configure(rknpu_dev);
	if (ret)
		return ret;
	ret = devm_pm_opp_of_add_table(dev);
	if (ret) {
		LOG_DEV_ERROR(dev, "failed to add OPP table: %d\n", ret);
		return ret;
	}

	if (!rknpu_dev->vdd)
		return -ENODEV;
	ret = k11c_vendor_prepare(rknpu_dev);
	if (ret)
		return ret;

	/* K11C normal profile: highest enabled vendor OPP up to 900 MHz.
	 * Do not change DT voltages/bin masks or enable the disabled 1 GHz OPP.
	 * Legacy single-OPP DTs continue using their assigned boot clock. */
	if (rknpu_dev->vendor) {
		initial_freq = 900000000;
		opp = dev_pm_opp_find_freq_floor(dev, &initial_freq);
	} else {
		initial_freq = clk_get_rate(rknpu_dev->clks[0].clk);
		opp = devfreq_recommended_opp(dev, &initial_freq, 0);
	}
	if (IS_ERR(opp)) {
		ret = PTR_ERR(opp);
		LOG_DEV_ERROR(dev, "failed to get recommended OPP: %d\n", ret);
		return ret;
	}
	ret = dev_pm_opp_get_supplies(opp, &supply);
	dev_pm_opp_put(opp);
	if (ret)
		return ret;
	if (!supply.u_volt_min || supply.u_volt_min > supply.u_volt_max)
		return -EINVAL;

	/* Preserve the normal request when cold policy temporarily selects less. */
	requested_freq = initial_freq;
	/* Selecting an OPP does not apply it. Do this even at the same clock. */
	ret = rknpu_dev->vendor ? k11c_vendor_set_rate(rknpu_dev, initial_freq) :
		dev_pm_opp_set_rate(dev, initial_freq);
	if (ret) {
		LOG_DEV_ERROR(dev, "failed to apply initial OPP: %d\n", ret);
		return ret;
	}
	/* Cold/OTP limits may select a lower OPP than the assigned boot clock.
	 * Verify against that applied OPP, not the pre-limit selection. */
	if (rknpu_dev->vendor) {
		initial_freq = rknpu_dev->current_freq;
		opp = devfreq_recommended_opp(dev, &initial_freq, 0);
		if (IS_ERR(opp))
			return PTR_ERR(opp);
		ret = dev_pm_opp_get_supplies(opp, &supply);
		dev_pm_opp_put(opp);
		if (ret)
			return ret;
	}
	if (!rknpu_dev->vendor) {
		ret = devm_add_action_or_reset(dev, rknpu_initial_opp_disable, dev);
		if (ret)
			return ret;
	}

	initial_uv = regulator_get_voltage(rknpu_dev->vdd);
	if (initial_uv < 0)
		return initial_uv;
	if (initial_uv < supply.u_volt_min || initial_uv > supply.u_volt_max) {
		LOG_DEV_ERROR(dev, "initial OPP voltage readback %d outside %lu..%lu uV\n",
			      initial_uv, supply.u_volt_min, supply.u_volt_max);
		return -ERANGE;
	}
	rknpu_dev->current_freq = clk_get_rate(rknpu_dev->clks[0].clk);
	if (!rknpu_dev->current_freq || rknpu_dev->current_freq > initial_freq)
		return -ERANGE;
	rknpu_dev->current_volt = initial_uv;
	LOG_DEV_INFO(dev, "K11C-NPU-OPP applied: target=%lu actual=%lu Hz volt=%d uV\n",
		     initial_freq, rknpu_dev->current_freq, initial_uv);

	dp->initial_freq = initial_freq;

	ret = devfreq_add_governor(&devfreq_rknpu_ondemand);
	if (ret) {
		LOG_DEV_ERROR(dev, "failed to add rknpu_ondemand governor: %d\n",
			      ret);
		return ret;
	}

	rknpu_dev->devfreq = devm_devfreq_add_device(dev, dp,
						      "rknpu_ondemand",
						      (void *)rknpu_dev);
	if (IS_ERR(rknpu_dev->devfreq)) {
		LOG_DEV_ERROR(dev, "failed to add devfreq device\n");
		ret = PTR_ERR(rknpu_dev->devfreq);
		rknpu_dev->devfreq = NULL;
		goto err_remove_governor;
	}

	rknpu_dev->ondemand_freq = rknpu_dev->vendor ? requested_freq :
		rknpu_dev->current_freq;

	rknpu_dev->devfreq->previous_freq = rknpu_dev->current_freq;
	if (rknpu_dev->devfreq->suspend_freq)
		rknpu_dev->devfreq->resume_freq = rknpu_dev->current_freq;
	rknpu_dev->devfreq->last_status.current_frequency =
		rknpu_dev->current_freq;
	rknpu_dev->devfreq->last_status.total_time = 1;
	rknpu_dev->devfreq->last_status.busy_time = 1;
	ret = k11c_vendor_monitor_start(rknpu_dev);
	if (ret)
		return ret;

	LOG_DEV_INFO(dev, "devfreq enabled, initial freq: %lu Hz, volt: %lu uV\n",
		     rknpu_dev->current_freq, rknpu_dev->current_volt);

	return 0;

err_remove_governor:
	devfreq_remove_governor(&devfreq_rknpu_ondemand);
	return ret;
}
EXPORT_SYMBOL(rknpu_devfreq_init);

void rknpu_devfreq_remove(struct rknpu_device *rknpu_dev)
{
	k11c_vendor_monitor_stop(rknpu_dev);
	if (rknpu_dev->devfreq) {
		devm_devfreq_remove_device(rknpu_dev->dev, rknpu_dev->devfreq);
		rknpu_dev->devfreq = NULL;
		devfreq_remove_governor(&devfreq_rknpu_ondemand);
	}
}
EXPORT_SYMBOL(rknpu_devfreq_remove);

int rknpu_devfreq_runtime_suspend(struct device *dev)
{
	return k11c_vendor_suspend(dev_get_drvdata(dev));
}
EXPORT_SYMBOL(rknpu_devfreq_runtime_suspend);

int rknpu_devfreq_runtime_resume(struct device *dev)
{
	return k11c_vendor_resume(dev_get_drvdata(dev));
}
EXPORT_SYMBOL(rknpu_devfreq_runtime_resume);
