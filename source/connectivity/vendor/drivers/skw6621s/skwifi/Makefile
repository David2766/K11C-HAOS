# SPDX-License-Identifier: GPL-2.0
obj-$(CONFIG_WLAN_VENDOR_SKW6316) += skw6316.o

skw6316-objs += skw_core.o
skw6316-objs += skw_iface.o
skw6316-objs += skw_cfg80211.o
skw6316-objs += skw_msg.o
skw6316-objs += skw_rx.o
skw6316-objs += skw_tx.o
skw6316-objs += skw_regd.o
skw6316-objs += skw_mlme.o
skw6316-objs += skw_timer.o
skw6316-objs += skw_log.o
skw6316-objs += skw_iw.o
skw6316-objs += skw_work.o
skw6316-objs += skw_mbssid.o
skw6316-objs += skw_dentry.o
skw6316-objs += skw_util.o
skw6316-objs += skw_config.o
skw6316-objs += skw_recovery.o
skw6316-objs += skw_vendor.o
skw6316-objs += trace.o
skw6316-objs += skw_db.o

# skw6316-$(CONFIG_SKW6316_CALIB_DPD) += skw_calib.o
# skw6316-$(CONFIG_SKW6316_EDMA) += skw_edma.o
skw6316-$(CONFIG_SKW6316_DFS_MASTER) += skw_dfs.o
skw6316-$(CONFIG_SKW6316_TDLS) += skw_tdls.o

ccflags-y += -DCONFIG_SKW6316_STA_SME_EXT
ccflags-y += -DCONFIG_SKW6316_SAP_SME_EXT
ccflags-y += -DCONFIG_SKW6316_SCAN_RANDOM_MAC
ccflags-y += -DCONFIG_SKW6316_TX_WORKQUEUE
ccflags-y += -DCONFIG_SKW6316_HIGH_PRIORITY
ccflags-y += -DCONFIG_SKW6316_CALIB_APPEND_BUS_ID
ccflags-y += -DCONFIG_SKW6316_CALIB_APPEND_MODULE_ID

ifneq ($(filter y,$(CONFIG_ANDROID_BINDER_IPC) $(CONFIG_SKW_ANDROID)),)
ccflags-y += -D__SKW_ANDROID__
endif

ccflags-y += -I$(srctree)/include/linux/platform_data/

ifneq ($(skw_extra_flags),)
ccflags-y += $(skw_extra_flags)
endif

ifneq ($(skw_extra_symbols),)
KBUILD_EXTRA_SYMBOLS += $(skw_extra_symbols)
endif

CFLAGS_trace.o := -I$(src)
$(obj)/skw_core.o : $(obj)/version.h

skw_abs_path := $(addprefix $(abspath $(srctree))/,$(filter-out /%,$(src)))$(filter /%,$(src))
$(obj)/version.h: $(skw_abs_path)/genver.pl
	@$(PERL) -s $(skw_abs_path)/genver.pl $@

clean-files := version.h
