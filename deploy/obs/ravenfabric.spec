# openSUSE Build Service (OBS) packaging for RavenFabric
# Submit to OBS: https://build.opensuse.org
#
# Usage:
#   osc checkout home:youruser/ravenfabric
#   cp deploy/obs/* home:youruser/ravenfabric/
#   cd home:youruser/ravenfabric
#   osc build openSUSE_Tumbleweed x86_64
#   osc commit -m "Initial ravenfabric package"

Name:           ravenfabric
Version:        0.1.4
Release:        1%{?dist}
Summary:        Secure remote execution and mesh networking agent
License:        AGPL-3.0-only
URL:            https://ravenfabric.io
Source0:        https://github.com/egkristi/RavenFabric/archive/v%{version}.tar.gz#/%{name}-%{version}.tar.gz

BuildRequires:  cargo >= 1.88
BuildRequires:  rust >= 1.88
BuildRequires:  gcc
BuildRequires:  openssl-devel
BuildRequires:  systemd-rpm-macros

%description
RavenFabric is a secure remote execution and mesh networking agent written in
Rust. It replaces Tailscale, Ansible, Salt, and similar tools with a single,
cryptographically verified binary. Features Noise XX mutual authentication,
deny-by-default policy, structured audit logging, and 30+ transport drivers
including LoRa, BLE, satellite, and mixnet.

%prep
%autosetup -n RavenFabric-%{version}

%build
cargo build --release --bin rf-agent --bin rf-relay --bin rf

%install
install -Dm755 target/release/rf-agent %{buildroot}%{_bindir}/rf-agent
install -Dm755 target/release/rf-relay %{buildroot}%{_bindir}/rf-relay
install -Dm755 target/release/rf %{buildroot}%{_bindir}/rf
install -Dm644 deploy/rf-agent.service %{buildroot}%{_unitdir}/rf-agent.service
install -Dm644 deploy/rf-relay.service %{buildroot}%{_unitdir}/rf-relay.service
install -Dm644 deploy/raven.toml.example %{buildroot}%{_sysconfdir}/ravenfabric/raven.toml.example
install -dm700 %{buildroot}%{_sysconfdir}/ravenfabric
install -dm755 %{buildroot}%{_localstatedir}/log/ravenfabric

%pre
getent group ravenfabric >/dev/null || groupadd -r ravenfabric
getent passwd ravenfabric >/dev/null || \
    useradd -r -g ravenfabric -d /etc/ravenfabric -s /sbin/nologin \
    -c "RavenFabric agent" ravenfabric
exit 0

%post
%systemd_post rf-agent.service

%preun
%systemd_preun rf-agent.service

%postun
%systemd_postun_with_restart rf-agent.service

%files
%license LICENSE
%doc README.md CHANGELOG.md
%{_bindir}/rf-agent
%{_bindir}/rf-relay
%{_bindir}/rf
%{_unitdir}/rf-agent.service
%{_unitdir}/rf-relay.service
%dir %attr(700,ravenfabric,ravenfabric) %{_sysconfdir}/ravenfabric
%config(noreplace) %{_sysconfdir}/ravenfabric/raven.toml.example
%dir %attr(755,ravenfabric,ravenfabric) %{_localstatedir}/log/ravenfabric

%changelog
* Fri May 09 2026 RavenFabric Maintainers <security@ravenfabric.io> - 0.1.4-1
- Version bump to 0.1.4
- First published release with cross-platform binaries

* Thu May 08 2026 RavenFabric Maintainers <security@ravenfabric.io> - 0.1.3-1
- Initial OBS package
- 50,000 LOC, 1,037 tests, 0 clippy warnings
- 30+ transport drivers including LoRa, BLE, satellite, mixnet
- Full Noise XX mutual authentication
- Deny-by-default policy engine
