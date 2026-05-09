# OpenWrt package Makefile for RavenFabric
# Build with: make package/ravenfabric/compile V=s

include $(TOPDIR)/rules.mk

PKG_NAME:=ravenfabric
PKG_VERSION:=0.1.4
PKG_RELEASE:=1
PKG_SOURCE:=$(PKG_NAME)-$(PKG_VERSION).tar.gz
PKG_SOURCE_URL:=https://github.com/egkristi/RavenFabric/archive/v$(PKG_VERSION)
PKG_HASH:=skip
PKG_BUILD_DIR:=$(BUILD_DIR)/RavenFabric-$(PKG_VERSION)
PKG_LICENSE:=MIT
PKG_LICENSE_FILES:=LICENSE

include $(INCLUDE_DIR)/package.mk

define Package/ravenfabric
  SECTION:=net
  CATEGORY:=Network
  TITLE:=Secure remote execution and mesh networking agent
  URL:=https://ravenfabric.io
  DEPENDS:=@(aarch64||arm||mipsel||x86_64)
endef

define Package/ravenfabric/description
  RavenFabric is a secure remote execution and mesh networking agent with
  Noise XX mutual authentication, deny-by-default policy enforcement,
  and structured audit logging. Single static binary, zero dependencies.
endef

define Package/ravenfabric/conffiles
/etc/ravenfabric/raven.toml
/etc/ravenfabric/policy.yaml
endef

define Build/Compile
	$(call Build/Compile/Cargo,rf-agent,--features minimal)
endef

define Package/ravenfabric/install
	$(INSTALL_DIR) $(1)/usr/bin
	$(INSTALL_BIN) $(PKG_BUILD_DIR)/target/$(CARGO_TARGET_DIR)/release/rf-agent $(1)/usr/bin/rf-agent

	$(INSTALL_DIR) $(1)/etc/ravenfabric
	$(INSTALL_CONF) ./files/raven.toml $(1)/etc/ravenfabric/raven.toml
	$(INSTALL_CONF) ./files/policy.yaml $(1)/etc/ravenfabric/policy.yaml

	$(INSTALL_DIR) $(1)/etc/init.d
	$(INSTALL_BIN) ./files/rf-agent.init $(1)/etc/init.d/rf-agent
endef

$(eval $(call BuildPackage,ravenfabric))
