Name:           arka-convert
Version:        0.1.0
Release:        1%{?dist}
Summary:        Offline file conversion and format wheel for ArkaOS
License:        GPL-3.0-or-later
URL:            https://github.com/thulasiramk-2310/arkaos
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cmake
BuildRequires:  gcc-c++
BuildRequires:  cmake(Qt6Gui)
BuildRequires:  cmake(Qt6Quick)
BuildRequires:  cmake(Qt6DBus)
BuildRequires:  cmake(Qt6QuickControls2)
BuildRequires:  cmake(KF6GlobalAccel)
BuildRequires:  cmake(KF6WindowSystem)
BuildRequires:  cmake(LayerShellQt)
BuildRequires:  systemd-rpm-macros

Requires:       bubblewrap
Requires:       ffmpeg-free
Requires:       python3-pillow
Requires:       poppler-utils
Requires:       ghostscript
Requires:       libnotify
Requires:       dbus-tools
Requires:       kf6-kio
Requires:       layer-shell-qt
Requires:       qt6-qtdeclarative
Recommends:     mat2

%description
arka-convert converts pictures, sound, video and PDFs next to the original,
inside a bubblewrap sandbox with no network. arka-wheel is the ring of formats
shown at the cursor (Meta+Alt+C, or Convert to... in Dolphin), and of edits
(Meta+Alt+E). arka-edit is the small window for crop, black out, background
and trim.

%prep
%autosetup

%build
%cmake -S convert
%cmake_build

%install
%cmake_install
install -Dm755 convert/arka-convert %{buildroot}%{_bindir}/arka-convert
for f in image pdf audio video; do
  install -Dm644 convert/arka-convert-$f.desktop %{buildroot}%{_datadir}/kio/servicemenus/arka-convert-$f.desktop
done
install -Dm644 convert/arka-wheel-servicemenu.desktop %{buildroot}%{_datadir}/kio/servicemenus/arka-wheel.desktop
install -Dm644 convert/arka-clean.desktop %{buildroot}%{_datadir}/kio/servicemenus/arka-clean.desktop
for f in image pdf pdf-merge media; do
  install -Dm644 convert/arka-edit-$f.desktop %{buildroot}%{_datadir}/kio/servicemenus/arka-edit-$f.desktop
done
install -Dm644 convert/org.arkaos.edit.desktop %{buildroot}%{_datadir}/applications/org.arkaos.edit.desktop
install -Dm644 convert/org.arkaos.wheel.desktop %{buildroot}%{_datadir}/applications/org.arkaos.wheel.desktop
%{_datadir}/applications/org.arkaos.edit.desktop
install -Dm644 convert/packaging/arka-wheel.service %{buildroot}%{_userunitdir}/arka-wheel.service
install -Dm644 convert/packaging/org.arkaos.Wheel.service %{buildroot}%{_datadir}/dbus-1/services/org.arkaos.Wheel.service

%post
%systemd_user_post arka-wheel.service

%preun
%systemd_user_preun arka-wheel.service

%files
%{_bindir}/arka-convert
%{_bindir}/arka-wheel
%{_bindir}/arka-edit
%{_datadir}/kio/servicemenus/arka-*.desktop
%{_datadir}/applications/org.arkaos.wheel.desktop
%{_userunitdir}/arka-wheel.service
%{_datadir}/dbus-1/services/org.arkaos.Wheel.service

%changelog
* Sun Oct 25 2026 Thulasi Ram K <thulasiramk.git@gmail.com> - 0.1.0-1
- First package
