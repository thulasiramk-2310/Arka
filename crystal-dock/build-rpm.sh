#!/bin/sh
# Rebuild Fedora's crystal-dock package with arkaos-look.patch. Runs in the
# dock-builder stage of the Containerfile; the RPM lands in /build/out/.
set -eu
cd /build
dnf download -q --source crystal-dock
rpm -i --nosignature crystal-dock-*.src.rpm
cp arkaos-look.patch ~/rpmbuild/SOURCES/
spec=~/rpmbuild/SPECS/crystal-dock.spec
sed -i -e 's/^Release:\( *\)\([^%]*\)/Release:\1\2.arkaos1/' \
       -e '/^Source0:/a Patch100:       arkaos-look.patch' \
       -e '/^BuildRequires:  cmake(LayerShellQt)/a BuildRequires:  cmake(KF6WindowSystem)' "$spec"
grep -q '^Patch100:' "$spec"
grep -q 'cmake(KF6WindowSystem)' "$spec"
dnf builddep -y -q "$spec"
rpmbuild -bb --nocheck --define 'debug_package %{nil}' "$spec" >/build/rpmbuild.log 2>&1 || { tail -40 /build/rpmbuild.log; exit 1; }
mkdir -p out
cp ~/rpmbuild/RPMS/x86_64/crystal-dock-[0-9]*.rpm out/
