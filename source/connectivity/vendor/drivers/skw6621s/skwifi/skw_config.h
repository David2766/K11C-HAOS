/* SPDX-License-Identifier: GPL-2.0 */

/******************************************************************************
 *
 * Copyright (C) 2020 SeekWave Technology Co.,Ltd.
 *
 * This program is free software; you can redistribute it and/or modify
 * it under the terms of version 2 of the GNU General Public License as
 * published by the Free Software Foundation;
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 ******************************************************************************/

#ifndef __SKW_CONFIG_H__
#define __SKW_CONFIG_H__

#include <linux/kernel.h>
#include <linux/etherdevice.h>

#define SKW_CFG_FLAG_OVERLAY_MODE           0
#define SKW_CFG_FLAG_STA_EXT                1
#define SKW_CFG_FLAG_SAP_EXT                2
#define SKW_CFG_FLAG_OFFCHAN_TX             4
#define SKW_CFG_FLAG_REPEATER               5
#define SKW_CFG_FLAG_P2P_DEV                6

struct skw_cfg_global {
	unsigned long flags;

	u8 mac[ETH_ALEN];
	u8 dma_addr_align;
	u8 reorder_timeout;
};

#define SKW_CFG_INTF_FLAG_INVALID           0
#define SKW_CFG_INTF_FLAG_LEGACY            1
struct skw_cfg_interface {
	char name[IFNAMSIZ];
	u8 mac[ETH_ALEN];
	u8 iftype;
	u8 inst;
	unsigned long flags;
};

struct skw_cfg_intf {
	struct skw_cfg_interface interface[4];
};

#define SKW_CFG_REGD_COUNTRY_CODE           0
#define SKW_CFG_REGD_SELF_MANAGED           1
#define SKW_CFG_REGD_IGNORE_USER            2
#define SKW_CFG_REGD_IGNORE_COUNTRY_IE      3

struct skw_cfg_regd {
	unsigned long flags;
	char country[2];
};

#define SKW_CFG_CALIB_STRICT_MODE           0
#define SKW_CFG_CALIB_CHIP_NAME             1
#define SKW_CFG_CALIB_PROJECT_NAME          2
#define SKW_CFG_CALIB_EXTRA_ID              3

struct skw_cfg_calib {
	unsigned long flags;

	char chip[16];
	char project[16];
};

struct skw_cfg_firmware {
	u32 link_loss_thrd;
	u32 noa_ratio_idx;
	u32 noa_ratio_en;
	u32 once_noa_en;
	u32 once_noa_pre;
	u32 once_noa_abs;
	u32 dot11k_disable;
	u32 dot11v_disable;
	u32 dot11r_disable;
	u32 offload_roaming_disable;
	u32 band_24ghz;
	u32 band_5ghz;
	u32 hdk_tst;
};

struct skw_config {
	struct skw_cfg_global global;
	struct skw_cfg_intf intf;
	struct skw_cfg_calib calib;
	struct skw_cfg_regd regd;
	struct skw_cfg_firmware fw;
};

void skw_update_config(struct device *dev, const char *name, struct skw_config *config);
#endif
