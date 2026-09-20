// SPDX-License-Identifier: GPL-2.0

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

#include <linux/ctype.h>
#include <linux/firmware.h>

#include "skw_util.h"
#include "skw_config.h"
#include "skw_log.h"
#include "skw_regd.h"

#define SKW_LINE_BUFF_LEN   128

struct skwifi_cfg {
	char *name;
	int (*parser)(struct skwifi_cfg *cfg, char *key, char *data);
	void *priv;
};

static struct skwifi_cfg *skw_cfg_match(struct skwifi_cfg *table, char *name)
{
	int i;
	struct skwifi_cfg *cfg = NULL;

	for (i = 0; table[i].name != NULL; i++) {
		if (!strcmp(name, table[i].name)) {
			cfg = &table[i];
			break;
		}
	}

	return cfg;
}

static void skw_parser(struct skwifi_cfg *table, const u8 *data, int size)
{
	char chr, token;
	char word[64] = {0};
	char skw_key[64] = {0};
	char skw_dat[256] = {0};
	int flags = 0;
	char comma[] = ",";
	int i, nr = 0, params = 0;
	struct skwifi_cfg *cfg = NULL;
	bool do_save, do_append, do_parse;

	for (token = 0, i = 0; i < size; i++) {
		chr = data[i];
		do_save = do_append = do_parse = false;

		switch (token) {
		case '%':
			if (chr == '%') {
				token = 0;
				word[nr] = '\0';

				cfg = skw_cfg_match(table, word);
			} else {
				do_save = true;
			}

			break;

		case '[':
			if (chr == ']') {
				token = 0;
				word[nr] = '\0';

				flags |= BIT(0);
				strlcpy(skw_key, word, sizeof(skw_key));
			} else {
				do_save = true;
			}

			break;

		case '"':
			if (chr == '"') {
				token = 0;
				do_append = true;
			} else {
				do_save = true;
			}

			break;

		case '<':
			if (chr == '<') {
				do_append = true;

			} else if (chr == '>') {
				token = 0;

				if (!nr)
					word[nr++] = '*';

				do_append = true;
			} else {
				if (isxdigit(chr) || chr == ',' ||
				    (tolower(chr) == 'x' && (nr && word[nr - 1] == '0')))
					do_save = true;
			}

			break;

		default:
			switch (chr) {
			case '%':
			case '[':
				nr = 0;
				memset(word, 0x0, sizeof(word));

				token = chr;
				do_parse = true;

				break;

			case '"':
			case '<':
				if (flags & BIT(1)) {
					nr = 0;
					memset(word, 0x0, sizeof(word));

					token = chr;
				}

				break;

			case '\n':
			case '\0':
				token = 0;
				do_parse = true;

				break;

			case '=':
				if (flags & BIT(0))
					flags |= BIT(1);

				break;

			default:
				// drop
				break;
			}

			break;
		}

		if (do_save) {
			if (nr < sizeof(word) - 1)
				word[nr++] = chr;
		} else if (do_append) {

			if (params++)
				strlcat(skw_dat, comma, sizeof(skw_dat));

			word[nr] = '\0';
			strlcat(skw_dat, word, sizeof(skw_dat));

			nr = 0;
			memset(word, 0x0, sizeof(word));

		} else if (do_parse) {
			if (cfg && flags & BIT(1) && params) {
				skw_detail("key: %s, data: %s\n", skw_key, skw_dat);
				cfg->parser(cfg, skw_key, skw_dat);
			}

			flags = 0;
			params = 0;
			do_parse = false;

			memset(skw_key, 0x0, sizeof(skw_key));
			memset(skw_dat, 0x0, sizeof(skw_dat));
		}
	}
}

static int skw_global_parser(struct skwifi_cfg *config, char *key, char *data)
{
	int ret;
	char *endp;
	struct skw_cfg_global *cfg = config->priv;

	if (cfg == NULL)
		return -EINVAL;

	if (!strcmp(key, "mac")) {
		u8 addr[ETH_ALEN] = {0};

		ret = sscanf(data, "0x%02x,0x%02x,0x%02x,0x%02x,0x%02x,0x%02x",
			     (int *)&addr[0], (int *)&addr[1], (int *)&addr[2],
			     (int *)&addr[3], (int *)&addr[4], (int *)&addr[5]);

		skw_dbg("ret: %d, addr: %pM\n", ret, addr);
		if (ret == ETH_ALEN && is_valid_ether_addr(addr))
			skw_ether_copy(cfg->mac, addr);

	} else if (!strcmp(key, "dma_addr_align")) {
		cfg->dma_addr_align = simple_strtol(data, &endp, 0);

	} else if (!strcmp(key, "reorder_timeout")) {
		cfg->reorder_timeout = simple_strtol(data, &endp, 0);

	} else if (!strcmp(key, "offchan_tx")) {
		switch (simple_strtol(data, &endp, 0)) {
		case 0:
			clear_bit(SKW_CFG_FLAG_OFFCHAN_TX, &cfg->flags);
			break;
		case 1:
			set_bit(SKW_CFG_FLAG_OFFCHAN_TX, &cfg->flags);
			break;
		default:
			break;
		}

	} else if (!strcmp(key, "overlay_mode")) {
		switch (simple_strtol(data, &endp, 0)) {
		case 0:
			clear_bit(SKW_CFG_FLAG_OVERLAY_MODE, &cfg->flags);
			break;
		case 1:
			set_bit(SKW_CFG_FLAG_OVERLAY_MODE, &cfg->flags);
			break;
		default:
			break;
		}

	} else {
		skw_dbg("unsupport key: %s\n", key);
	}

	return 0;
}

static int skw_firmware_parser(struct skwifi_cfg *config, char *key, char *data)
{
	int ret;
	char *endp;
	struct skw_cfg_firmware *cfg = config->priv;

	if (cfg == NULL)
		return -EINVAL;

	if (!strcmp(key, "link_loss_thrd")) {
		cfg->link_loss_thrd = simple_strtol(data, &endp, 0);

		skw_dbg("link_loss_thrd: %d\n", cfg->link_loss_thrd);
	} else if (!strcmp(key, "go_noa_ratio_idx")) {
		cfg->noa_ratio_en = simple_strtol(data, &endp, 0);

		if (cfg->noa_ratio_en)
			ret = sscanf(data, "%d,%d",
				&cfg->noa_ratio_en, &cfg->noa_ratio_idx);

		skw_dbg("ret:%d noa_ratio_en: %d, idx:%d\n", ret,
			 cfg->noa_ratio_en, cfg->noa_ratio_idx);
	} else if (!strcmp(key, "force_go_once_noa")) {
		cfg->once_noa_en = simple_strtol(data, &endp, 0);

		if (cfg->once_noa_en)
			ret = sscanf(data, "%d,%d,%d",
				&cfg->once_noa_en, &cfg->once_noa_pre, &cfg->once_noa_abs);

		skw_dbg("ret:%d once_noa_en: %d, pre:%d, abs:%d\n", ret, cfg->once_noa_en,
			 cfg->once_noa_pre, cfg->once_noa_abs);
	} else if (!strcmp(key, "offload_roaming_disable")) {
		cfg->offload_roaming_disable = simple_strtol(data, &endp, 0);

		skw_dbg("offload_roaming_disable: %d\n", cfg->offload_roaming_disable);
	} else if (!strcmp(key, "24ghz_bandwidth")) {
		cfg->band_24ghz = simple_strtol(data, &endp, 0);

		skw_dbg("24ghz_bandwidth: %d\n", cfg->band_24ghz);
	} else if (!strcmp(key, "5ghz_bandwidth")) {
		cfg->band_5ghz = simple_strtol(data, &endp, 0);

		skw_dbg("5ghz_bandwidth: %d\n", cfg->band_5ghz);
	} else if (!strcmp(key, "hdk_test")) {
		cfg->hdk_tst = simple_strtol(data, &endp, 0);

		skw_dbg("hdk_test: %d\n", cfg->hdk_tst);
	} else {
		skw_dbg("unsupport key: %s\n", key);
	}

	return 0;
}

static int skw_intf_parser(struct skwifi_cfg *config, char *key, char *data)
{
	int ret;
	char *endp;
	int i, nr_params;
	char mode[64] = {0};
	char inst[64] = {0};
	char flags[64] = {0};
	char mac[64] = {0};
	char ifname[32] = {0};
	u8 addr[ETH_ALEN] = {0};
	// struct skw_cfg_intf *intf = config->priv;

	/* "ifname",<mode>,<inst>,<flags>,<mac> */
	nr_params = sscanf(data, "%16[^,],%32[^,],%32[^,],%32[^,],%s",
			   ifname, mode, inst, flags, mac);

	skw_detail("key: %s, data: %s, nr_params: %d\n", key, data, nr_params);

	for (i = 0; i < nr_params; i++)
		switch (i) {
		/* interface name */
		case 0:
			skw_dbg("ifname: %s\n", ifname);
			break;

		/* interface mode */
		case 1:
			skw_dbg("mode: %ld\n", simple_strtol(mode, &endp, 0));
			break;

		/* interface instance */
		case 2:
			skw_dbg("inst: %ld\n", simple_strtol(inst, &endp, 0));
			break;

		/* interface flags */
		case 3:
			skw_dbg("flags: 0x%lx\n", simple_strtol(flags, &endp, 0));
			break;

		/* interface mac */
		case 4:
			ret = sscanf(mac, "0x%02x,0x%02x,0x%02x,0x%02x,0x%02x,0x%02x",
				     (int *)&addr[0], (int *)&addr[1],
				     (int *)&addr[2], (int *)&addr[3],
				     (int *)&addr[4], (int *)&addr[5]);

			skw_dbg("ret: %d, addr: %pM\n", ret, addr);
			break;
		}

	return 0;
}

static int skw_calib_parser(struct skwifi_cfg *config, char *key, char *data)
{
	char *endp;
	struct skw_cfg_calib *calib = config->priv;

	if (calib == NULL)
		return -EINVAL;

	if (!strcmp(key, "strict_mode")) {
		switch (simple_strtol(data, &endp, 0)) {
		case 0:
			clear_bit(SKW_CFG_CALIB_STRICT_MODE, &calib->flags);
			break;
		case 1:
			set_bit(SKW_CFG_CALIB_STRICT_MODE, &calib->flags);
			break;
		default:
			break;
		}

	} else if (!strcmp(key, "chip")) {
		strlcpy(calib->chip, data, sizeof(calib->chip) - 1);

	} else if (!strcmp(key, "project")) {
		strlcpy(calib->project, data, sizeof(calib->project) - 1);

	} else {
		skw_dbg("unsupport key: %s\n", key);
	}

	return 0;
}

static int skw_regdom_parser(struct skwifi_cfg *config, char *key, char *data)
{
	struct skw_cfg_regd *regd = config->priv;

	if (regd == NULL)
		return -EINVAL;

	if (!strcmp(key, "country")) {
		if (strlen(data) == 2)
			memcpy(regd->country, data, 2);

	} else {
		skw_dbg("unsupport key: %s\n", key);
	}

	return 0;
}

static int skw_roam_parser(struct skwifi_cfg *config, char *key, char *data)
{
	return 0;
}

static struct skwifi_cfg  g_cfg_table[] = {
	{
		.name = "global",
		.parser = skw_global_parser,
	},
	{
		.name = "interface",
		.parser = skw_intf_parser,
	},
	{
		.name = "calib",
		.parser = skw_calib_parser,
	},
	{
		.name = "regdom",
		.parser = skw_regdom_parser,
	},
	{
		.name = "roaming",
		.parser = skw_roam_parser,
	},
	{
		.name = "firmware",
		.parser = skw_firmware_parser,
	},
	{
		.name = NULL,
		.parser = NULL,
		.priv = NULL,
	}
};

void skw_update_config(struct device *dev, const char *name, struct skw_config *config)
{
	int i;
	const struct firmware *fw;

	if (request_firmware_direct(&fw, name, dev))
		return;

	skw_dbg("load %s successful\n", name);

	for (i = 0; g_cfg_table[i].name != NULL; i++) {
		if (!strcmp(g_cfg_table[i].name, "global"))
			g_cfg_table[i].priv = &config->global;
		else if (!strcmp(g_cfg_table[i].name, "interface"))
			g_cfg_table[i].priv = &config->intf;
		else if (!strcmp(g_cfg_table[i].name, "calib"))
			g_cfg_table[i].priv = &config->calib;
		else if (!strcmp(g_cfg_table[i].name, "regdom"))
			g_cfg_table[i].priv = &config->regd;
		else if (!strcmp(g_cfg_table[i].name, "roaming"))
			g_cfg_table[i].priv = NULL;
		else if (!strcmp(g_cfg_table[i].name, "firmware"))
			g_cfg_table[i].priv = &config->fw;
		else {
			g_cfg_table[i].priv = NULL;

			skw_warn("section: %s not support\n", g_cfg_table[i].name);
		}
	}

	skw_parser(g_cfg_table, fw->data, fw->size);

	release_firmware(fw);
}
