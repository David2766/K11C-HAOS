// SPDX-License-Identifier: GPL-2.0-only
/*
 * Rockchip RK3568 OTP read path, used by RK3566 as in the K11C BSP.
 * Copyright (c) 2018 Rockchip Electronics Co., Ltd.
 * Original author: Finley Xiao <finley.xiao@rock-chips.com>
 *
 * RK3568-only extraction of drivers/nvmem/rockchip-otp.c. The ECC-enabled
 * read sequence is retained. No OTP programming callback or write parameter
 * is included. Controller writes below select/read data, not program fuses.
 * The separate driver name avoids colliding with HAOS's built-in OTP driver,
 * which does not support rockchip,rk3568-otp in Linux 6.18.39.
 */
#include <linux/clk.h>
#include <linux/delay.h>
#include <linux/io.h>
#include <linux/iopoll.h>
#include <linux/module.h>
#include <linux/mutex.h>
#include <linux/nvmem-provider.h>
#include <linux/of.h>
#include <linux/platform_device.h>
#include <linux/reset.h>
#include <linux/slab.h>

#define OTPC_SBPI_CTRL             0x0020
#define OTPC_SBPI_CMD_VALID_PRE    0x0024
#define OTPC_LOCK_CTRL             0x0050
#define OTPC_USER_CTRL             0x0100
#define OTPC_USER_ADDR             0x0104
#define OTPC_USER_ENABLE           0x0108
#define OTPC_USER_QP               0x0120
#define OTPC_USER_Q                0x0124
#define OTPC_INT_STATUS            0x0304
#define OTPC_SBPI_CMD0_OFFSET      0x1000
#define OTPC_SBPI_CMD1_OFFSET      0x1004
#define OTPC_USER_ADDR_MASK        GENMASK(31, 16)
#define OTPC_USE_USER              BIT(0)
#define OTPC_USE_USER_MASK         BIT(16)
#define OTPC_USER_FSM_ENABLE       BIT(0)
#define OTPC_USER_FSM_ENABLE_MASK  BIT(16)
#define OTPC_LOCK                  BIT(0)
#define OTPC_LOCK_MASK             BIT(16)
#define OTPC_SBPI_DONE             BIT(1)
#define OTPC_USER_DONE             BIT(2)
#define SBPI_DAP_ADDR              0x02
#define SBPI_DAP_ADDR_SHIFT        8
#define SBPI_DAP_ADDR_MASK         GENMASK(31, 24)
#define SBPI_CMD_VALID_MASK        GENMASK(31, 16)
#define SBPI_DAP_CMD_WRF           0xc0
#define SBPI_DAP_REG_ECC           0x3a
#define SBPI_ECC_ENABLE            0x00
#define SBPI_ECC_DISABLE           0x09
#define SBPI_ENABLE               BIT(0)
#define SBPI_ENABLE_MASK          BIT(16)
#define OTPC_TIMEOUT              10000
#define RK3568_NBYTES             2
#define RK3568_OTP_SIZE           0x80

struct rockchip_otp {
	struct device *dev;
	void __iomem *base;
	struct clk_bulk_data clks[4];
	int num_clks;
	struct mutex mutex;
	struct reset_control *rst;
};

static int rockchip_otp_reset(struct rockchip_otp *otp)
{
	int ret;

	ret = reset_control_assert(otp->rst);
	if (ret) {
		dev_err(otp->dev, "failed to assert otp phy %d\n", ret);
		return ret;
	}

	udelay(2);

	ret = reset_control_deassert(otp->rst);
	if (ret) {
		dev_err(otp->dev, "failed to deassert otp phy %d\n", ret);
		return ret;
	}

	return 0;
}

static int px30_otp_wait_status(struct rockchip_otp *otp, u32 flag)
{
	u32 status = 0;
	int ret;

	ret = readl_poll_timeout_atomic(otp->base + OTPC_INT_STATUS, status,
					(status & flag), 1, OTPC_TIMEOUT);
	if (ret)
		return ret;

	/* clean int status */
	writel(flag, otp->base + OTPC_INT_STATUS);

	return 0;
}

static int px30_otp_ecc_enable(struct rockchip_otp *otp, bool enable)
{
	int ret = 0;

	writel(SBPI_DAP_ADDR_MASK | (SBPI_DAP_ADDR << SBPI_DAP_ADDR_SHIFT),
	       otp->base + OTPC_SBPI_CTRL);

	writel(SBPI_CMD_VALID_MASK | 0x1, otp->base + OTPC_SBPI_CMD_VALID_PRE);
	writel(SBPI_DAP_CMD_WRF | SBPI_DAP_REG_ECC,
	       otp->base + OTPC_SBPI_CMD0_OFFSET);
	if (enable)
		writel(SBPI_ECC_ENABLE, otp->base + OTPC_SBPI_CMD1_OFFSET);
	else
		writel(SBPI_ECC_DISABLE, otp->base + OTPC_SBPI_CMD1_OFFSET);

	writel(SBPI_ENABLE_MASK | SBPI_ENABLE, otp->base + OTPC_SBPI_CTRL);

	ret = px30_otp_wait_status(otp, OTPC_SBPI_DONE);
	if (ret < 0)
		dev_err(otp->dev, "timeout during ecc_enable\n");

	return ret;
}

static int rk3568_otp_read(void *context, unsigned int offset, void *val,
			   size_t bytes)
{
	struct rockchip_otp *otp = context;
	unsigned int addr_start, addr_end, addr_offset, addr_len;
	unsigned int otp_qp;
	u32 out_value;
	u8 *buf;
	int ret = 0, i = 0;

	addr_start = rounddown(offset, RK3568_NBYTES) / RK3568_NBYTES;
	addr_end = roundup(offset + bytes, RK3568_NBYTES) / RK3568_NBYTES;
	addr_offset = offset % RK3568_NBYTES;
	addr_len = addr_end - addr_start;

	buf = kzalloc(array3_size(addr_len, RK3568_NBYTES, sizeof(*buf)),
		      GFP_KERNEL);
	if (!buf)
		return -ENOMEM;

	ret = clk_bulk_prepare_enable(otp->num_clks, otp->clks);
	if (ret < 0) {
		dev_err(otp->dev, "failed to prepare/enable clks\n");
		goto out;
	}

	ret = rockchip_otp_reset(otp);
	if (ret) {
		dev_err(otp->dev, "failed to reset otp phy\n");
		goto disable_clks;
	}

	ret = px30_otp_ecc_enable(otp, true);
	if (ret < 0) {
		dev_err(otp->dev, "rockchip_otp_ecc_enable err\n");
		goto disable_clks;
	}

	writel(OTPC_USE_USER | OTPC_USE_USER_MASK, otp->base + OTPC_USER_CTRL);
	udelay(5);
	while (addr_len--) {
		writel(addr_start++ | OTPC_USER_ADDR_MASK,
		       otp->base + OTPC_USER_ADDR);
		writel(OTPC_USER_FSM_ENABLE | OTPC_USER_FSM_ENABLE_MASK,
		       otp->base + OTPC_USER_ENABLE);
		ret = px30_otp_wait_status(otp, OTPC_USER_DONE);
		if (ret < 0) {
			dev_err(otp->dev, "timeout during read setup\n");
			goto read_end;
		}
		otp_qp = readl(otp->base + OTPC_USER_QP);
		if (((otp_qp & 0xc0) == 0xc0) || (otp_qp & 0x20)) {
			ret = -EIO;
			dev_err(otp->dev, "ecc check error during read setup\n");
			goto read_end;
		}
		out_value = readl(otp->base + OTPC_USER_Q);
		memcpy(&buf[i], &out_value, RK3568_NBYTES);
		i += RK3568_NBYTES;
	}

	memcpy(val, buf + addr_offset, bytes);

read_end:
	writel(0x0 | OTPC_USE_USER_MASK, otp->base + OTPC_USER_CTRL);
disable_clks:
	clk_bulk_disable_unprepare(otp->num_clks, otp->clks);
out:
	kfree(buf);

	return ret;
}

static int k11c_otp_read(void *context, unsigned int offset, void *val,
			 size_t bytes)
{
	struct rockchip_otp *otp = context;
	int ret;

	if (!bytes || offset >= RK3568_OTP_SIZE || bytes > RK3568_OTP_SIZE - offset)
		return -EINVAL;
	mutex_lock(&otp->mutex);
	ret = rk3568_otp_read(context, offset, val, bytes);
	mutex_unlock(&otp->mutex);
	return ret;
}

static int k11c_otp_probe(struct platform_device *pdev)
{
	struct device *dev = &pdev->dev;
	struct rockchip_otp *otp;
	struct nvmem_config config = {};
	static const char * const names[] = { "usr", "sbpi", "apb", "phy" };
	int i, ret;

	if (!of_machine_is_compatible("kickpi,k11c") ||
	    !of_machine_is_compatible("rockchip,rk3566"))
		return -ENODEV;
	otp = devm_kzalloc(dev, sizeof(*otp), GFP_KERNEL);
	if (!otp)
		return -ENOMEM;
	otp->dev = dev;
	mutex_init(&otp->mutex);
	otp->base = devm_platform_ioremap_resource(pdev, 0);
	if (IS_ERR(otp->base))
		return PTR_ERR(otp->base);
	otp->num_clks = ARRAY_SIZE(names);
	for (i = 0; i < otp->num_clks; ++i)
		otp->clks[i].id = names[i];
	ret = devm_clk_bulk_get(dev, otp->num_clks, otp->clks);
	if (ret)
		return ret;
	/* Vendor 5.10 pulses the PHY reset before each ECC read transaction. */
	otp->rst = devm_reset_control_array_get_optional_exclusive(dev);
	if (IS_ERR(otp->rst))
		return PTR_ERR(otp->rst);
	config.dev = dev;
	config.name = "k11c-rk3568-otp";
	config.owner = THIS_MODULE;
	config.read_only = true;
	config.root_only = true;
	config.add_legacy_fixed_of_cells = true;
	config.reg_read = k11c_otp_read;
	config.size = RK3568_OTP_SIZE;
	config.stride = 1;
	config.word_size = 1;
	config.priv = otp;
	return PTR_ERR_OR_ZERO(devm_nvmem_register(dev, &config));
}

static const struct of_device_id k11c_otp_match[] = {
	{ .compatible = "rockchip,rk3568-otp" },
	{}
};
MODULE_DEVICE_TABLE(of, k11c_otp_match);
static struct platform_driver k11c_otp_driver = {
	.probe = k11c_otp_probe,
	.driver = {
		.name = "k11c-rk3568-otp",
		.of_match_table = k11c_otp_match,
	},
};
module_platform_driver(k11c_otp_driver);
MODULE_DESCRIPTION("K11C RK3568 read-only OTP provider (vendor ECC read path)");
MODULE_VERSION("r22.1");
MODULE_LICENSE("GPL v2");
