// SPDX-License-Identifier: GPL-2.0+
/*
 * KickPi K11C late PHY preparation.
 *
 * The onboard Maxio PHY starts with a clock mode that prevents the RK3566
 * GMAC DMA reset from completing.  Prepare the verified PHY revision after
 * MDIO registration and remove only the Linux reset that would undo it.
 */

#include <dm.h>
#include <event.h>
#include <miiphy.h>
#include <reset.h>
#include <asm/global_data.h>
#include <linux/delay.h>
#include <linux/libfdt.h>
#include <linux/mii.h>

DECLARE_GLOBAL_DATA_PTR;

#define K11C_COMPATIBLE              "kickpi,k11c"
#define K11C_ETH_NAME                "ethernet@fe010000"
#define K11C_PHY_NODE                "/ethernet@fe010000/mdio/ethernet-phy@0"
#define K11C_PHY_ADDR                0
#define K11C_MAXIO_PHY_ID            0x7b744411
#define K11C_MAXIO_PAGE_REG          0x1f
#define K11C_MAXIO_CLOCK_PAGE        0x0d92
#define K11C_MAXIO_CLOCK_REG         0x02
#define K11C_MAXIO_CLOCK_MODE        0x200a

static int k11c_maxio_set_clock_mode(struct mii_dev *bus, u16 mode)
{
	int ret;

	ret = bus->write(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
			 K11C_MAXIO_PAGE_REG, K11C_MAXIO_CLOCK_PAGE);
	if (ret)
		return ret;

	ret = bus->write(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
			 K11C_MAXIO_CLOCK_REG, mode);
	bus->write(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
		   K11C_MAXIO_PAGE_REG, 0);

	return ret;
}

static int k11c_prepare_maxio_phy(void)
{
	struct reset_ctl reset;
	struct udevice *eth_dev;
	struct mii_dev *bus;
	u32 phy_id;
	int old_mode = -1;
	int node;
	int id1;
	int id2;
	int mode;
	int ret;

	if (!of_machine_is_compatible(K11C_COMPATIBLE))
		return 0;

	ret = uclass_get_device_by_name(UCLASS_ETH, K11C_ETH_NAME, &eth_dev);
	if (ret) {
		printf("K11C: Ethernet device unavailable (%d)\n", ret);
		return 0;
	}

	ret = reset_get_by_name(eth_dev, "stmmaceth", &reset);
	if (ret) {
		printf("K11C: GMAC reset unavailable (%d)\n", ret);
		return 0;
	}

	ret = reset_deassert(&reset);
	if (ret)
		goto out_free;
	udelay(10);

	bus = miiphy_get_dev_by_name(eth_dev->name);
	if (!bus) {
		ret = -ENODEV;
		goto out_restore_reset;
	}

	/* This PHY exposes its valid ID only after an ID2 write. */
	ret = bus->write(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE, MII_PHYSID2, 0);
	if (ret)
		goto out_restore_reset;

	id1 = bus->read(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE, MII_PHYSID1);
	id2 = bus->read(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE, MII_PHYSID2);
	if (id1 < 0 || id2 < 0) {
		ret = -EIO;
		goto out_restore_reset;
	}

	phy_id = ((u32)(id1 & 0xffff) << 16) | (id2 & 0xffff);
	if (phy_id != K11C_MAXIO_PHY_ID) {
		printf("K11C: unsupported PHY %08x; leaving DT unchanged\n",
		       phy_id);
		ret = -ENODEV;
		goto out_restore_reset;
	}

	ret = bus->write(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
			 K11C_MAXIO_PAGE_REG, K11C_MAXIO_CLOCK_PAGE);
	if (ret)
		goto out_restore_reset;

	old_mode = bus->read(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
			     K11C_MAXIO_CLOCK_REG);
	if (old_mode < 0) {
		ret = -EIO;
		goto out_restore_page;
	}

	ret = bus->write(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
			 K11C_MAXIO_CLOCK_REG, K11C_MAXIO_CLOCK_MODE);
	if (ret)
		goto out_restore_mode;

	mode = bus->read(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
			 K11C_MAXIO_CLOCK_REG);
	if (mode != K11C_MAXIO_CLOCK_MODE) {
		ret = mode < 0 ? mode : -EIO;
		goto out_restore_mode;
	}

	ret = bus->write(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
			 K11C_MAXIO_PAGE_REG, 0);
	if (ret)
		goto out_restore_mode;

	node = fdt_path_offset(gd->fdt_blob, K11C_PHY_NODE);
	if (node < 0) {
		ret = node;
		goto out_restore_mode;
	}

	ret = fdt_delprop((void *)gd->fdt_blob, node, "reset-gpios");
	if (ret && ret != -FDT_ERR_NOTFOUND)
		goto out_restore_mode;

	printf("K11C: Maxio PHY %08x prepared (D92:02=%04x)\n",
	       phy_id, K11C_MAXIO_CLOCK_MODE);
	reset_free(&reset);
	return 0;

out_restore_mode:
	if (old_mode >= 0)
		k11c_maxio_set_clock_mode(bus, old_mode);
	else
		bus->write(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
			   K11C_MAXIO_PAGE_REG, 0);
	goto out_restore_reset;

out_restore_page:
	bus->write(bus, K11C_PHY_ADDR, MDIO_DEVAD_NONE,
		   K11C_MAXIO_PAGE_REG, 0);
out_restore_reset:
	reset_assert(&reset);
out_free:
	printf("K11C: Maxio PHY preparation failed (%d)\n", ret);
	reset_free(&reset);
	return 0;
}
EVENT_SPY_SIMPLE(EVT_LAST_STAGE_INIT, k11c_prepare_maxio_phy);
