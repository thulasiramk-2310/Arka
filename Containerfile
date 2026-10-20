ARG FEDORA_VERSION=44
# Stage 1: build arkad as a static musl binary
FROM docker.io/rust:alpine AS builder
RUN apk add --no-cache musl-dev
COPY arkad/ /build/
COPY arka-shell/arka-shell-common/ /arka-shell/arka-shell-common/
WORKDIR /build
RUN cargo build --locked --release --target x86_64-unknown-linux-musl

# arka-pulsed: the reliability daemon, also a static musl binary (its D-Bus
# surface uses the same zbus stack as arkad). Built after arkad so a change to
# one does not invalidate the other's layer. --features dbus turns on the
# optional daemon; the core stays zero-dep without it.
COPY arka-pulse/ /arka-pulse/
RUN cargo build --locked --release --target x86_64-unknown-linux-musl \
      --features dbus --manifest-path /arka-pulse/Cargo.toml

# Stage 2: build the arka-shell GTK4 apps (glibc required, not musl)
FROM docker.io/fedora:${FEDORA_VERSION} AS shell-builder
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y -q gtk4-devel gtk4-layer-shell-devel libadwaita-devel rust cargo gcc pkgconf-pkg-config
COPY arka-shell/ /build/
WORKDIR /build
RUN cargo build --locked --release

# Stage 2b: the dock. Fedora's crystal-dock source package rebuilt with
# crystal-dock/arkaos-look.patch (blurred background, label bubble, divider
# before Trash).
FROM docker.io/fedora:${FEDORA_VERSION} AS dock-builder
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y -q rpm-build 'dnf-command(builddep)' 'dnf-command(download)'
COPY crystal-dock/arkaos-look.patch crystal-dock/build-rpm.sh /build/
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked sh /build/build-rpm.sh

# Stage 2c: the format wheel (convert/wheel), a small Qt Quick program that
# waits in the session and shows a ring of formats at the cursor.
FROM docker.io/fedora:${FEDORA_VERSION} AS wheel-builder
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y -q cmake gcc-c++ extra-cmake-modules qt6-qtbase-devel qt6-qtdeclarative-devel kf6-kglobalaccel-devel kf6-kwindowsystem-devel layer-shell-qt-devel
COPY convert/wheel/ /build/
RUN cmake -S /build -B /build/out -DCMAKE_BUILD_TYPE=Release && cmake --build /build/out -j"$(nproc)"

# Stage 2d: VARA, version control (github.com/thulasiramk-2310/vara), built
# from its v0.4.3 release, with the web files of its Hub. The checkout must be
# exactly the release commit, so a moved tag fails the build instead of
# changing what ships. Pure Go, static.
FROM docker.io/library/golang:1.25-alpine AS vara-builder
RUN apk add --no-cache git
ARG VARA_TAG=v0.4.3
ARG VARA_COMMIT=a39f0c1b7d9dfcab2ef7dcd53792d8dcfa9a7154
RUN git clone --quiet --depth 1 --branch "$VARA_TAG" https://github.com/thulasiramk-2310/vara /src && \
    test "$(git -C /src rev-parse HEAD)" = "$VARA_COMMIT" && \
    cd /src && CGO_ENABLED=0 go build -trimpath -ldflags "-s -w -X main.version=${VARA_TAG#v}" -o /out/vara ./cmd/vara && \
    cp -r /src/web /out/hub && rm -f /out/hub/README.md

# Staging stages for the small, often-edited files and the built binaries,
# laid out at their final paths. Each COPY into the 11 GB bootc image costs
# about a minute and a half (the layer diff walks the whole image), so the
# final stage takes each tree in ONE step instead of ~35. COPY into an empty
# stage is near-instant.
FROM scratch AS files
COPY firewalld/arkaos.xml /usr/lib/firewalld/zones/arkaos.xml
COPY kernel/10-arkaos-hardening.toml /usr/lib/bootc/kargs.d/10-arkaos-hardening.toml
COPY kernel/60-arkaos-hardening.conf /usr/lib/sysctl.d/60-arkaos-hardening.conf
COPY kernel/20-arkaos-recovery.toml /usr/lib/bootc/kargs.d/20-arkaos-recovery.toml
COPY arka-arm-boot-counter /usr/libexec/arka-arm-boot-counter
COPY arka-stale-trigger-check /usr/libexec/arka-stale-trigger-check
COPY autostart/arka-privacy-dots.desktop /etc/xdg/autostart/arka-privacy-dots.desktop
COPY crystal-dock/arkaos-dock.desktop /etc/xdg/autostart/arkaos-dock.desktop
COPY crystal-dock/arka-dock-start /usr/libexec/arka-dock-start
COPY crystal-dock/panel_1.conf /etc/skel/.config/crystal-dock/KDE/panel_1.conf
COPY crystal-dock/appearance.conf /etc/skel/.config/crystal-dock/KDE/appearance.conf
COPY crystal-dock/panel_1.conf /etc/skel/.config/crystal-dock/generic/panel_1.conf
COPY crystal-dock/appearance.conf /etc/skel/.config/crystal-dock/generic/appearance.conf
COPY arka-rtc-mode /usr/libexec/arka-rtc-mode
COPY systemd/arka-rtc-mode.service /usr/lib/systemd/system/arka-rtc-mode.service
COPY arka-dns-after-clock /usr/libexec/arka-dns-after-clock
COPY systemd/arka-dns-after-clock.service /usr/lib/systemd/system/arka-dns-after-clock.service
COPY tmpfiles/arkaos-arkasetup.conf /usr/lib/tmpfiles.d/arkaos-arkasetup.conf
COPY systemd/greenboot-healthcheck.service.d/10-arkaos-stale-trigger.conf /usr/lib/systemd/system/greenboot-healthcheck.service.d/10-arkaos-stale-trigger.conf
COPY systemd/greenboot-set-rollback-trigger.service.d/10-arkaos-stop-at-shutdown.conf /usr/lib/systemd/system/greenboot-set-rollback-trigger.service.d/10-arkaos-stop-at-shutdown.conf
COPY arka-apply-look /usr/libexec/arka-apply-look
COPY convert/arka-convert /usr/bin/arka-convert
COPY convert/arka-convert-image.desktop /usr/share/kio/servicemenus/arka-convert-image.desktop
COPY convert/arka-convert-pdf.desktop /usr/share/kio/servicemenus/arka-convert-pdf.desktop
COPY convert/arka-convert-audio.desktop /usr/share/kio/servicemenus/arka-convert-audio.desktop
COPY convert/arka-convert-video.desktop /usr/share/kio/servicemenus/arka-convert-video.desktop
COPY convert/arka-wheel-servicemenu.desktop /usr/share/kio/servicemenus/arka-wheel.desktop
COPY convert/arka-clean.desktop /usr/share/kio/servicemenus/arka-clean.desktop
COPY convert/org.arkaos.wheel.desktop /usr/share/applications/org.arkaos.wheel.desktop
COPY convert/arka-wheel-autostart.desktop /etc/xdg/autostart/arka-wheel.desktop
COPY systemd/vara-hub.service /usr/lib/systemd/user/vara-hub.service
COPY plasma-style/panel /usr/share/plasma/desktoptheme/arkaos-dark
COPY plasma-style/panel /usr/share/plasma/desktoptheme/arkaos-light
COPY arkad/org.arka.arkad.policy /usr/share/polkit-1/actions/org.arka.arkad.policy
COPY arkaos-firefox /usr/bin/firefox-sandbox
COPY arkad/arkad.service /usr/lib/systemd/system/arkad.service
COPY arkad/arkad.toml /etc/arkad/arkad.toml
COPY arkad/org.arka.arkad.conf /etc/dbus-1/system.d/org.arka.arkad.conf
COPY arkad/org.arka.arkad.service /usr/share/dbus-1/system-services/org.arka.arkad.service
COPY arka-pulse/arka-pulsed.service /usr/lib/systemd/system/arka-pulsed.service
COPY arka-pulse/org.arka.pulse.conf /etc/dbus-1/system.d/org.arka.pulse.conf
COPY arka-pulse/org.arka.pulse.service /usr/share/dbus-1/system-services/org.arka.pulse.service
COPY docs/RELIABILITY-ARKA-PULSE.md /usr/share/doc/arkaos/RELIABILITY-ARKA-PULSE.md
COPY firefox-policies.json /usr/lib64/firefox/distribution/policies.json

FROM scratch AS bins
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/arkad /usr/bin/arkad
COPY --from=builder /arka-pulse/target/x86_64-unknown-linux-musl/release/arka-pulsed /usr/bin/arka-pulsed
COPY --from=shell-builder /build/target/release/arka-dashboard /usr/bin/arka-dashboard
COPY --from=shell-builder /build/target/release/arka-wifi      /usr/bin/arka-wifi
COPY --from=shell-builder /build/target/release/arka-update    /usr/bin/arka-update
COPY --from=shell-builder /build/target/release/arka-hotkeys   /usr/bin/arka-hotkeys
COPY --from=shell-builder /build/target/release/arka-capsule   /usr/bin/arka-capsule
COPY --from=shell-builder /build/target/release/arka-perms    /usr/bin/arka-perms
COPY --from=shell-builder /build/target/release/arka-settings /usr/bin/arka-settings-gtk
COPY --from=shell-builder /build/target/release/arka-welcome  /usr/bin/arka-welcome
COPY --from=shell-builder /build/target/release/arka-sound      /usr/bin/arka-sound
COPY --from=shell-builder /build/target/release/arka-bluetooth  /usr/bin/arka-bluetooth
COPY --from=shell-builder /build/target/release/arka-privacy-tray /usr/bin/arka-privacy-tray
COPY --from=shell-builder /build/target/release/arka-oobe /usr/bin/arka-oobe
COPY --from=shell-builder /build/target/release/arka-privacy-dots /usr/bin/arka-privacy-dots
COPY --from=wheel-builder /build/out/arka-wheel /usr/bin/arka-wheel
COPY --from=vara-builder /out/vara /usr/bin/vara
COPY --from=vara-builder /out/hub /usr/share/vara/hub

# Stage 3: bootc image
FROM quay.io/fedora/fedora-bootc:${FEDORA_VERSION}

RUN mkdir -p /etc/NetworkManager/conf.d && \
    printf '[device]\nwifi.scan-rand-mac-address=yes\n\n[connection]\nwifi.cloned-mac-address=random\nethernet.cloned-mac-address=random\n' \
      > /etc/NetworkManager/conf.d/00-arkaos-mac-random.conf

RUN mkdir -p /usr/lib/bootc/install && \
    printf '[install.filesystem.root]\ntype = "xfs"\n' \
      > /usr/lib/bootc/install/00-defaults.toml

# Phase 3a: UKI toolchain + systemd-boot EFI binary
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y systemd-ukify systemd-boot-unsigned binutils

# Switch kernel-install to UKI layout
RUN printf 'layout=uki\nuki_generator=ukify\n' > /etc/kernel/install.conf

# Phase 4: bubblewrap-sandboxed Firefox
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y firefox bubblewrap

# Graphical session: full KDE Plasma spin + SDDM login manager.
# Plasma provides the panel, launcher, settings, file manager, window
# management and login — replacing the custom Hyprland shell. The ArkaOS
# privacy apps (dashboard, capsule, settings) run as regular Plasma apps.
# rootfiles is excluded: it ships /root dotfiles that already exist in the
# bootc base, so its cpio unpack fails the whole transaction. It's cosmetic.
# retries=25 + fastestmirror ride out the transient Fedora mirror hiccups that
# otherwise fail this 1500-package transaction ("All mirrors were tried").
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y --setopt=retries=25 --setopt=fastestmirror=True \
    --setopt=max_parallel_downloads=10 --exclude=rootfiles \
    @kde-desktop-environment sddm \
    pipewire wireplumber pipewire-pulseaudio xorg-x11-server-Xwayland \
    libadwaita flatpak xdg-user-dirs brightnessctl \
    bluez bluez-tools zram-generator-defaults

# Enable SDDM (started once firstboot flips the default to graphical.target).
# Default to the Plasma Wayland session.
# Fedora 44's KDE install makes Plasma Login Manager (plasmalogin) the display
# manager. ArkaOS keeps SDDM for now (its login theme is an SDDM theme), so
# --force repoints display-manager.service at sddm.
RUN systemctl enable --force sddm.service && \
    mkdir -p /etc/sddm.conf.d && \
    printf '[General]\nDisplayServer=wayland\n\n[Wayland]\nSessionDir=/usr/share/wayland-sessions\n' \
      > /etc/sddm.conf.d/10-wayland.conf





# Firefox managed privacy policy: Enhanced Tracking Protection (strict) with
# cryptomining + fingerprinting blocking. Placed in Firefox's app-dir under /usr
# (which the bwrap sandbox binds read-only) — NOT /etc/firefox, which the sandbox
# deliberately does not expose. Backs the dashboard's "Blocks fingerprinting
# scripts" claim; verify at runtime via about:policies. Copied in the late
# block below so editing it doesn't bust the KDE cache.

# mako notification config + skel/Pictures for screenshots
RUN mkdir -p /etc/skel/.config/mako /etc/skel/Pictures && \
    printf '[global]\nbackground-color=#0d0d1aff\ntext-color=#d0dff0ff\nborder-color=#1a3a5aff\nborder-radius=8\nborder-size=1\nfont=Liberation Sans 12\nwidth=320\nmargin=10\npadding=12\ndefault-timeout=4000\n' \
      > /etc/skel/.config/mako/config

# Preserve the distribution's password-quality policy for installed accounts.

# Timezone: Asia/Kolkata (UTC+5:30)
RUN ln -sf /usr/share/zoneinfo/Asia/Kolkata /etc/localtime && \
    echo "Asia/Kolkata" > /etc/timezone

# Silence console: kernel printk level=1 (EMERG+ALERT only) + no systemd status lines
RUN mkdir -p /etc/sysctl.d /etc/systemd/system.conf.d && \
    printf 'kernel.printk = 1 4 1 7\n' \
      > /etc/sysctl.d/00-arkaos-quiet.conf && \
    printf '[Manager]\nShowStatus=no\n' \
      > /etc/systemd/system.conf.d/00-arkaos-quiet.conf

# First-boot setup wizard + settings utility
COPY arkaos-firstboot         /usr/libexec/arkaos-firstboot
COPY arkaos-firstboot.service /usr/lib/systemd/system/arkaos-firstboot.service
COPY arkaos-settings          /usr/bin/arkaos-settings
RUN chmod 755 /usr/libexec/arkaos-firstboot /usr/bin/arkaos-settings && \
    echo '%wheel ALL=(ALL) NOPASSWD: /usr/bin/arkaos-settings' \
      > /etc/sudoers.d/99-arkaos-settings

# First boot must land on the CONSOLE (multi-user.target) so the first-run flow
# runs and creates the account BEFORE any display manager. The wizard then flips
# the default to graphical.target and reboots into SDDM. Without this the image
# defaults to graphical.target and SDDM shows an empty, unusable login on boot 1.
RUN systemctl set-default multi-user.target

# ── Graphical first-run wizard (OOBE) ────────────────────────────────────────
# A Wayland compositor cannot get a seat as a pre-login *system* service, so the
# wizard runs inside a real logind LOGIN SESSION — exactly how DP1's sway worked.
# A dedicated `arkasetup` user auto-logs in on tty1 (first boot only), logind
# grants it seat0 (DRM+input), and its login shell launches cage + the arka-oobe
# GTK wizard. arka-oobe runs unprivileged and creates the account only through a
# tight NOPASSWD sudo helper. If cage can't start, it falls back to the TUI.
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y --setopt=retries=25 cage
RUN useradd -m -s /bin/bash arkasetup && passwd -l arkasetup && \
    printf 'arkasetup ALL=(root) NOPASSWD: /usr/libexec/arkaos-oobe-apply, /usr/libexec/arkaos-firstboot\n' \
      > /etc/sudoers.d/95-arkaos-oobe && \
    mkdir -p /etc/systemd/system/getty@tty1.service.d && \
    printf '[Unit]\nConditionPathExists=!/var/lib/arkaos-firstboot-done\n[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin arkasetup --skip-login --noissue --nohostname --nohints --noclear %%I $TERM\n' \
      > /etc/systemd/system/getty@tty1.service.d/arkasetup-autologin.conf && \
    printf 'if [ "$(tty)" = /dev/tty1 ] && [ ! -e /var/lib/arkaos-firstboot-done ]; then\n  printf "\\033[2J\\033[H\\033[?25l"\n  export GSK_RENDERER=cairo GDK_BACKEND=wayland\n  cage -- /usr/bin/arka-oobe >"$HOME/arka-oobe.log" 2>&1 || { printf "\\033[?25h"; sudo -n /usr/libexec/arkaos-firstboot; }\nfi\n' \
      > /home/arkasetup/.bash_profile && \
    touch /home/arkasetup/.hushlogin && \
    chown arkasetup:arkasetup /home/arkasetup/.bash_profile /home/arkasetup/.hushlogin

# ArkaOS default wallpapers, one per look: the Arka triangle as hairlines on a
# quiet field, no text, rendered at 4K so HiDPI panels stay sharp. default.png
# (the dark one) backs the login and lock screens. Users can change them.
COPY wallpapers/make-default.sh /tmp/make-default.sh
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y -q ImageMagick && \
    mkdir -p /usr/share/arka/wallpapers && \
    sh /tmp/make-default.sh /usr/share/arka/wallpapers/default-light.png 3840 2160 light && \
    sh /tmp/make-default.sh /usr/share/arka/wallpapers/default-dark.png 3840 2160 dark && \
    cp /usr/share/arka/wallpapers/default-dark.png /usr/share/arka/wallpapers/default.png && \
    rm /tmp/make-default.sh && \
    dnf remove -y -q ImageMagick

# arka-tools .desktop files for launcher search
COPY desktop-files/ /usr/share/applications/

# ── ArkaWM: brand KDE Plasma as ArkaOS ──────────────────────────────────────
# Inter typeface (ArkaOS design system) + the ArkaOS Plasma color scheme.
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y rsms-inter-fonts
COPY ArkaOS.colors /usr/share/color-schemes/ArkaOS.colors
COPY ArkaOSLight.colors /usr/share/color-schemes/ArkaOSLight.colors

# Design-system defaults for every new user (read from /etc/skel on account
# creation): ArkaOS color scheme, green #16C784 accent, Inter font — applied
# from the first frame.
RUN mkdir -p /etc/skel/.config && \
    printf '[General]\nColorScheme=ArkaOS\nAccentColor=22,199,132\naccentColorFromWallpaper=false\nfont=Inter,10,-1,5,50,0,0,0,0,0\nfixed=JetBrains Mono,10,-1,5,50,0,0,0,0,0\nmenuFont=Inter,10,-1,5,50,0,0,0,0,0\nsmallestReadableFont=Inter,8,-1,5,50,0,0,0,0,0\ntoolBarFont=Inter,10,-1,5,50,0,0,0,0,0\n\n[KDE]\nwidgetStyle=Breeze\nLookAndFeelPackage=org.kde.breezedark.desktop\n\n[Icons]\nTheme=Arka\n\n[Sounds]\nTheme=Arka\n\n[WM]\nactiveFont=Inter,10,-1,5,75,0,0,0,0,0\n' \
      > /etc/skel/.config/kdeglobals

# First-login one-shot: apply the ArkaOS wallpaper + accent via the official
# Plasma CLI tools (reliable across versions), then disable itself.
# (arka-plasma-firstrun and arka-layout.js are installed in the late block.)
RUN mkdir -p /etc/skel/.config/autostart /usr/share/arka && \
    printf '[Desktop Entry]\nType=Application\nName=ArkaOS Branding\nExec=/usr/libexec/arka-plasma-firstrun\nX-KDE-autostart-phase=2\nOnlyShowIn=KDE\nNoDisplay=true\n' \
      > /etc/skel/.config/autostart/arka-plasma-firstrun.desktop

# Privacy Activity tray (item 1) — StatusNotifierItem that makes arkad's
# enforcement visible. Read-only; reads arkad D-Bus + /var/log/arkaos/privacy.jsonl.
RUN printf '[Desktop Entry]\nType=Application\nName=ArkaOS Privacy Activity\nExec=/usr/bin/arka-privacy-tray\nX-KDE-autostart-phase=2\nOnlyShowIn=KDE\nNoDisplay=true\n' \
      > /etc/skel/.config/autostart/arka-privacy-tray.desktop

# Custom ArkaOS SDDM login theme (dark, green accent, fade-in animation).
COPY sddm-theme-arkaos/ /usr/share/sddm/themes/arkaos/
RUN printf '[Theme]\nCurrent=arkaos\n' > /etc/sddm.conf.d/30-arka-theme.conf && \
    printf '[Users]\nHideUsers=arkasetup\n' > /etc/sddm.conf.d/31-arka-hide-setup.conf
# The login theme follows the desktop accent (theme.conf accent=auto), read
# from the sddm user's colour scheme. Seed it with the ArkaOS scheme so a fresh
# install matches the desktop default. "C" copies only when the file is
# missing, so "Apply Plasma Settings" (which rewrites it) always wins.
RUN printf 'd /var/lib/sddm/.config 0755 sddm sddm -\nC /var/lib/sddm/.config/kdeglobals - - - - /usr/share/color-schemes/ArkaOS.colors\nz /var/lib/sddm/.config/kdeglobals 0644 sddm sddm -\n' \
      > /usr/lib/tmpfiles.d/arka-sddm-accent.conf

# Lock screen: ArkaOS wallpaper behind the (already dark+green) Plasma locker.
RUN printf '[Greeter][Wallpaper][org.kde.image][General]\nImage=/usr/share/arka/wallpapers/default.png\n' \
      > /etc/skel/.config/kscreenlockerrc

# (The desktop wallpaper and panel layout now come from the ArkaOS global
# themes built in the late block, so the first frame is already ArkaOS.)

# Keep expensive optional effects off by default on modest graphics hardware.
# Users can enable them through Plasma Desktop Effects.
RUN printf '[Plugins]\nwobblywindowsEnabled=false\nmagiclampEnabled=false\nkwin4_effect_scaleEnabled=true\n' \
      > /etc/skel/.config/kwinrc

# Filename/content indexing is opt-in; desktop search still launches apps.
RUN printf '[Basic Settings]\nIndexing-Enabled=false\n' > /etc/skel/.config/baloofilerc

# Brand the session name shown in SDDM.
RUN if [ -f /usr/share/wayland-sessions/plasma.desktop ]; then \
      sed -i 's/^Name=.*/Name=ArkaOS Desktop/' /usr/share/wayland-sessions/plasma.desktop; \
    fi

# ── Arka brand identity: icons, cursor, sounds, boot splash ─────────────────

# OS identity: About screens, neofetch, bug reports all say ArkaOS + version.
# ID stays "fedora" — tools key repo/firmware behavior off it.
RUN sed -i \
      -e 's/^NAME=.*/NAME="ArkaOS"/' \
      -e 's/^PRETTY_NAME=.*/PRETTY_NAME="ArkaOS 0.1.0-dp1 (Developer Preview 1)"/' \
      -e 's/^VERSION=.*/VERSION="0.1.0-dp1"/' \
      -e 's|^HOME_URL=.*|HOME_URL="https://github.com/thulasiramk-2310/Arka"|' \
      -e 's|^BUG_REPORT_URL=.*|BUG_REPORT_URL="https://github.com/thulasiramk-2310/Arka/issues"|' \
      /usr/lib/os-release && \
    echo "0.1.0-dp1" > /usr/lib/arkaos-release

# ArkaOS has its own welcome; KDE's Konqi tour would fight the identity.
# kded6's kded_plasma_welcome module launches it whenever plasma-welcomerc
# lacks [General] LastSeenVersion (verified via strings on the .so), so remove
# the package (rpm -e --test confirms no reverse deps) AND seed the config
# system-wide + in skel in case a future image update pulls it back in.
# mcelog: /dev/mcelog exists even in VMs, so gate on virtualization
# (the daemon starts then dies on the virtual CPU otherwise).
RUN rpm -e plasma-welcome && \
    printf '[General]\nLastSeenVersion=99.0.0\n' \
      > /etc/xdg/plasma-welcomerc && \
    printf '[General]\nLastSeenVersion=99.0.0\n' \
      > /etc/skel/.config/plasma-welcomerc && \
    mkdir -p /etc/systemd/system/mcelog.service.d && \
    printf '[Unit]\nConditionVirtualization=false\n' \
      > /etc/systemd/system/mcelog.service.d/10-hardware-only.conf

# Arka icon theme (dark plates, green line glyphs; inherits breeze-dark).
COPY arka-icons/ /usr/share/icons/Arka/

# The KDE spin already ships power profiles (tuned-ppd — same D-Bus API as
# power-profiles-daemon, which CONFLICTS with it) plus kio-extras and
# ffmpegthumbs for file previews. Only the plymouth script engine is missing.
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y --setopt=retries=25 plymouth-plugin-script

# ArkaCursor: white pointer with green outline, generated at build; every
# other shape inherits from breeze_cursors. ArkaSound: soft sine-based event
# sounds. Plymouth splash images. All need ImageMagick/xcursorgen/sox once.
COPY plymouth-theme-arkaos/ /usr/share/plymouth/themes/arkaos/
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y -q ImageMagick xcursorgen sox liberation-sans-fonts && \
    # -- cursor ---------------------------------------------------------------
    mkdir -p /tmp/cur /usr/share/icons/ArkaCursor/cursors && \
    cd /tmp/cur && \
    magick -size 96x96 xc:none \
      -fill "#f5f7fa" -stroke "#16c784" -strokewidth 6 \
      -draw "polygon 12,8 12,84 34,62 48,92 60,86 46,58 72,58" ptr96.png && \
    for s in 24 32 48; do \
      magick ptr96.png -resize ${s}x${s} ptr_${s}.png; \
    done && \
    printf '24 3 2 ptr_24.png\n32 4 3 ptr_32.png\n48 6 4 ptr_48.png\n' > ptr.cfg && \
    xcursorgen ptr.cfg /usr/share/icons/ArkaCursor/cursors/left_ptr && \
    magick -size 96x96 xc:none \
      -stroke "#f5f7fa" -strokewidth 6 -fill none \
      -draw "line 48,14 48,82 line 36,14 60,14 line 36,82 60,82" ibeam96.png && \
    for s in 24 32 48; do \
      magick ibeam96.png -resize ${s}x${s} ibeam_${s}.png; \
    done && \
    printf '24 12 12 ibeam_24.png\n32 16 16 ibeam_32.png\n48 24 24 ibeam_48.png\n' > txt.cfg && \
    xcursorgen txt.cfg /usr/share/icons/ArkaCursor/cursors/text && \
    cd /usr/share/icons/ArkaCursor/cursors && \
    for a in default arrow top_left_arrow left_arrow; do ln -sf left_ptr $a; done && \
    for a in xterm ibeam; do ln -sf text $a; done && \
    printf '[Icon Theme]\nName=ArkaCursor\nComment=ArkaOS cursor theme\nInherits=breeze_cursors\n' \
      > /usr/share/icons/ArkaCursor/index.theme && \
    # -- sounds ---------------------------------------------------------------
    mkdir -p /usr/share/sounds/Arka/stereo && cd /usr/share/sounds/Arka/stereo && \
    sox -n -r 44100 desktop-login.wav        synth 0.35 sine 523:784  fade t 0.02 0.35 0.15 vol 0.30 && \
    sox -n -r 44100 message-new-instant.wav  synth 0.18 sine 880      fade t 0.01 0.18 0.09 vol 0.25 && \
    sox -n -r 44100 dialog-error.wav         synth 0.25 sine 185      fade t 0.01 0.25 0.10 vol 0.30 && \
    sox -n -r 44100 camera-shutter.wav       synth 0.06 whitenoise    fade t 0.005 0.06 0.03 vol 0.20 && \
    sox -n -r 44100 device-added.wav         synth 0.15 sine 587:880  fade t 0.01 0.15 0.07 vol 0.25 && \
    sox -n -r 44100 device-removed.wav       synth 0.15 sine 880:587  fade t 0.01 0.15 0.07 vol 0.25 && \
    sox -n -r 44100 battery-low.wav          synth 0.30 sine 330:262  fade t 0.01 0.30 0.12 vol 0.30 && \
    sox -n -r 44100 audio-volume-change.wav  synth 0.08 sine 660      fade t 0.005 0.08 0.04 vol 0.20 && \
    cp message-new-instant.wav bell.wav && \
    printf '[Sound Theme]\nName=Arka\nComment=ArkaOS sound theme\nInherits=freedesktop\nDirectories=stereo\n\n[stereo]\nOutputProfile=stereo\n' \
      > /usr/share/sounds/Arka/index.theme && \
    # -- plymouth splash images ------------------------------------------------
    cd /usr/share/plymouth/themes/arkaos && \
    sh make-assets.sh && \
    rm make-assets.sh && \
    rm -rf /tmp/cur && \
    dnf remove -y -q ImageMagick xcursorgen sox

# Default cursor theme for new users.
RUN printf '[Mouse]\ncursorTheme=ArkaCursor\n' > /etc/skel/.config/kcminputrc

# Activate the boot splash: set the theme, rebuild the initramfs so plymouth
# carries it, and add rhgb+quiet kargs so the splash actually shows. Disk
# images get console=ttyS0 from bootc-image-builder, and plymouth falls back to
# text ("details") whenever a serial console is present, so tell it to leave
# serial consoles alone: splash on screen, boot log still on the serial port.
RUN plymouth-set-default-theme arkaos && \
    KVER=$(ls /usr/lib/modules | head -1) && \
    dracut --force --no-hostonly /usr/lib/modules/$KVER/initramfs.img $KVER && \
    mkdir -p /usr/lib/bootc/kargs.d && \
    printf 'kargs = ["rhgb", "quiet", "plymouth.ignore-serial-consoles"]\n' > /usr/lib/bootc/kargs.d/10-arkaos-splash.toml

# Keep the boot screen clean from power-on to setup: hide the GRUB menu (it
# still waits 1s; hold Esc to show it; bootupd's menu_show_once still forces
# it when set) and keep kernel/udev chatter off the screen.
RUN printf 'set timeout_style=hidden\nset timeout=1\n' \
      > /usr/lib/bootupd/grub2-static/configs.d/12_arka_menu_hidden.cfg && \
    printf 'kargs = ["loglevel=3", "rd.udev.log_level=3"]\n' > /usr/lib/bootc/kargs.d/11-arkaos-quiet.toml

# Build the UKI only after the final initramfs and OS identity are ready.
# Build UKI from the final installed kernel+initramfs — no embedded cmdline
RUN set -e; \
    KVER=$(ls /usr/lib/modules | head -1); \
    ukify build \
      --linux  /usr/lib/modules/${KVER}/vmlinuz \
      --initrd /usr/lib/modules/${KVER}/initramfs.img \
      --stub   /usr/lib/systemd/boot/efi/linuxx64.efi.stub \
      --os-release /etc/os-release \
      --output /usr/lib/modules/${KVER}/${KVER}.efi; \
    echo "UKI built: $(ls -lh /usr/lib/modules/${KVER}/${KVER}.efi)"

# Fedora 44's KDE enables its own first-run wizard (plasma-setup) on top of
# ArkaOS's OOBE, so new users were asked to set up the machine twice. Mask it:
# ArkaOS's wizard creates the account. (Placed here, not by the SDDM step, so
# an existing build cache is not invalidated.)
RUN systemctl mask plasma-setup.service

# Monospace font the desktop config already names (kdeglobals fixed=) and the
# setup screen types its "hello world" in.
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y jetbrains-mono-fonts

# Arka Light icons: the same Arka app icons, falling back to Breeze's
# light-background set instead of breeze-dark, so tray and launcher glyphs stay
# visible on the light look. arka-plasma-firstrun switches to it for Light.
RUN cp -a /usr/share/icons/Arka /usr/share/icons/Arka-Light && \
    sed -i -e 's/^Name=.*/Name=Arka Light/' -e 's/^Inherits=.*/Inherits=breeze/' \
           -e 's/^Comment=.*/Comment=ArkaOS icon theme for the light look/' \
           /usr/share/icons/Arka-Light/index.theme && \
    gtk-update-icon-cache -q -f /usr/share/icons/Arka-Light 2>/dev/null || true

# Update status for unprivileged apps (bootc status needs root): written to
# /run/arkaos/bootc-status.json at boot, every 10 min, and when deployments change.
COPY systemd/arka-bootc-status.service systemd/arka-bootc-status.timer systemd/arka-bootc-status.path /usr/lib/systemd/system/
RUN systemctl enable arka-bootc-status.timer arka-bootc-status.path

# ArkaOS wallpapers as real KDE wallpaper packages, and the system default
# (plasmarc [Wallpapers] defaultWallpaperTheme): the very first frame of a new
# desktop is ArkaOS's, instead of KDE's "Next" until a login script swaps it.
RUN for look in Dark Light; do \
      l=$(echo $look | tr A-Z a-z); d=/usr/share/wallpapers/ArkaOS-$look; \
      mkdir -p $d/contents/images && \
      ln -sf /usr/share/arka/wallpapers/default-$l.png $d/contents/images/3840x2160.png && \
      printf '{"KPlugin":{"Id":"ArkaOS-%s","Name":"ArkaOS %s","License":"GPL-3.0-or-later","Authors":[{"Name":"ArkaOS"}]}}\n' "$look" "$look" > $d/metadata.json; \
    done && \
    printf '[Wallpapers]\ndefaultWallpaperTheme=ArkaOS-Dark\ndefaultFileSuffix=.png\ndefaultWidth=3840\ndefaultHeight=2160\n' > /etc/xdg/plasmarc

# Setup helper and first-login script (here so editing them is a cheap rebuild).
COPY arkaos-oobe-apply /usr/libexec/arkaos-oobe-apply
COPY arka-plasma-firstrun /usr/libexec/arka-plasma-firstrun
COPY arka-layout.js /usr/share/arka/arka-layout.js

# Updates come from ghcr.io/thulasiramk-2310/arka, built and signed by the
# Image workflow. Only images signed with the ArkaOS key are accepted from
# there; a pull of an unsigned or differently signed image fails before
# anything is installed. Other registries keep Fedora's default policy.
COPY cosign.pub /etc/pki/containers/arka.pub
COPY containers/arka.yaml /etc/containers/registries.d/arka.yaml
RUN python3 -c 'import json; p="/etc/containers/policy.json"; c=json.load(open(p)); \
c.setdefault("transports", {}).setdefault("docker", {})["ghcr.io/thulasiramk-2310/arka"] = \
[{"type": "sigstoreSigned", "keyPath": "/etc/pki/containers/arka.pub", "signedIdentity": {"type": "matchRepository"}}]; \
json.dump(c, open(p, "w"), indent=4)'
RUN chmod 755 /usr/libexec/arkaos-oobe-apply /usr/libexec/arka-plasma-firstrun

# Touchpads scroll the content with the fingers ("natural" scrolling), as on
# macOS, Windows and GNOME; KDE's own default is the opposite. KWin reads this
# default at login for every touchpad; System Settings can still turn it off.
RUN printf '[Libinput][Defaults][Touchpad]\nNaturalScroll=true\n' > /etc/xdg/kcminputrc

# ArkaOS global themes ("look-and-feel"), built from Breeze's. Plasma reads a
# new desktop's wallpaper and panel layout from the global theme, so these
# make the very first frame ArkaOS: colours, Plasma style, icons, cursor,
# wallpaper, top bar + dock. No splash between login and desktop. Every part
# stays changeable in System Settings.
RUN for v in dark light; do \
      if [ $v = dark ]; then base=org.kde.breezedark.desktop; scheme=ArkaOS; style=arkaos-dark; icons=Arka; wall=ArkaOS-Dark; \
      else base=org.kde.breeze.desktop; scheme=ArkaOSLight; style=arkaos-light; icons=Arka-Light; wall=ArkaOS-Light; fi; \
      d=/usr/share/plasma/look-and-feel/org.arkaos.$v.desktop; \
      cp -a /usr/share/plasma/look-and-feel/$base $d && \
      python3 -c 'import json,sys; p=sys.argv[1]; m=json.load(open(p)); k=m["KPlugin"]; v=sys.argv[2]; \
k.update(Id="org.arkaos."+v+".desktop", Name="ArkaOS "+v.title(), Description="The ArkaOS "+v+" look"); \
[k.pop(x) for x in list(k) if x.startswith(("Name[","Description["))]; json.dump(m,open(p,"w"),indent=1)' $d/metadata.json $v && \
      f=$d/contents/defaults && \
      kwriteconfig6 --file $f --group kdeglobals --group General --key ColorScheme $scheme && \
      kwriteconfig6 --file $f --group kdeglobals --group Icons --key Theme $icons && \
      kwriteconfig6 --file $f --group plasmarc --group Theme --key name $style && \
      kwriteconfig6 --file $f --group Wallpaper --key Image $wall && \
      kwriteconfig6 --file $f --group kcminputrc --group Mouse --key cursorTheme ArkaCursor && \
      kwriteconfig6 --file $f --group ksplashrc --group KSplash --key Theme None && \
      kwriteconfig6 --file $f --group ksplashrc --group KSplash --key Engine none && \
      cp /usr/share/arka/arka-layout.js $d/contents/layouts/org.kde.plasma.desktop-layout.js || exit 1; \
    done && \
    kwriteconfig6 --file /etc/skel/.config/kdeglobals --group KDE --key LookAndFeelPackage org.arkaos.dark.desktop && \
    kwriteconfig6 --file /etc/xdg/kdeglobals --group KDE --key LookAndFeelPackage org.arkaos.dark.desktop && \
    printf '[KSplash]\nEngine=none\nTheme=None\n' > /etc/skel/.config/ksplashrc

# Firewall on by default: the ArkaOS zone rejects unsolicited incoming
# connections (Fedora KDE's default zone allows ports 1025-65535). arkad
# checks it and the Privacy dashboard shows it.
# greenboot: health checks after each boot. If a new version fails them, the
# computer restarts to retry (3 boots) and then goes back to the previous
# version on its own. greenboot-set-rollback-trigger records that version from
# its ExecStop during shutdown, but upstream's unit (v0.16.4 and main) has no
# shutdown conflict, so systemd never stopped it and ExecStop never ran:
# "Boot counter exhausted but no next-deployment-id set" (VM test 2026-10-11).
# Our drop-in adds Conflicts/Before=final.target (files stage); with it the
# same test rolled back unattended and a healthy update stayed.
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y -q \
      firewalld greenboot greenboot-default-health-checks && \
    systemctl enable firewalld.service && \
    for u in $(rpm -ql greenboot | grep -E '^/usr/lib/systemd/system/[^/]+\.service$' | xargs -n1 basename); do \
      systemctl enable "$u" && echo "greenboot: enabled $u"; \
    done
# Small files and binaries: one COPY each from the staging stages above.
# Keep each file's purpose next to its line in the "files" stage; what the
# RUN below does with them:
#  - firewall: ArkaOS zone (rejects unsolicited incoming connections; Fedora
#    KDE's default zone allows ports 1025-65535) made the default. arkad checks
#    it and the Privacy dashboard shows it.
#  - kernel hardening: boot options via bootc kargs.d, runtime via sysctl.d.
#  - updates download in the background but never restart the computer on
#    their own: Fedora's bootc-fetch-apply-updates timer runs `bootc upgrade
#    --apply`, which reboots. Keep the timer, drop --apply; the Updates app
#    shows "Restart needed" and the user picks the moment.
#  - light login screen: the same SDDM theme with look=light and the light
#    wallpaper; arkaos-oobe-apply selects it when the user picks Light.
#  - Firefox runs through the bubblewrap sandbox wrapper.
#  - arkad and arka-pulsed (read-only org.arka.pulse, mirrors arkad) enabled.
#  - time and names after a wrong clock: chrony also asks four time servers by address
#    (so it works without DNS) and arka-dns-after-clock restarts systemd-resolved once
#    when the clock jumps. Fixes "Server not found" on a dead-battery laptop (Nokia).
#  - arka-rtc-mode (first boot): when the firmware lists Windows Boot Manager, read
#    the hardware clock as local time like Windows; otherwise ArkaOS shows a time
#    5 h 30 min off. Changes how Linux reads the clock only, never Windows.
#  - ArkaOS Plasma styles (arkaos-dark/-light): Breeze's colours with ArkaOS
#    panels (24px corners, 1px border, soft shadow; plasma-style/). Plasma
#    takes every other element from its default style.
# Runtime library for the privacy dots overlay, and the dock (Crystal Dock with
# the ArkaOS patch, its own layer-shell window, so icons can grow above it;
# see crystal-dock/).
# A late layer: the big package layers above stay cached.
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked --mount=type=bind,from=dock-builder,source=/build/out,target=/tmp/dock \
    dnf install --setopt=keepcache=True -y -q gtk4-layer-shell /tmp/dock/crystal-dock-*.rpm
# "Convert to" in the Dolphin right-click menu (convert/arka-convert): ffmpeg for
# sound and video (Fedora's build includes MP3, AAC and H.264 through OpenH264),
# Pillow for pictures, poppler for PDF pages, mat2 for "Remove hidden details".
# The tool runs them with no network.
# The format wheel (arka-wheel) needs Qt Quick, KGlobalAccel, KWindowSystem and
# LayerShellQt, which Plasma already brings; listed so they stay.
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked \
    dnf install --setopt=keepcache=True -y -q ffmpeg-free python3-pillow poppler-utils mat2 \
      qt6-qtdeclarative kf6-kglobalaccel kf6-kwindowsystem layer-shell-qt
# Git, offered next to VARA in setup's Advanced options.
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked \
    dnf install --setopt=keepcache=True -y -q git-core
COPY --from=files / /
RUN sed -i 's/^DefaultZone=.*/DefaultZone=arkaos/' /etc/firewalld/firewalld.conf && \
    grep -q '^DefaultZone=arkaos' /etc/firewalld/firewalld.conf && \
    chmod 755 /usr/libexec/arka-apply-look /usr/libexec/arka-arm-boot-counter /usr/libexec/arka-stale-trigger-check /usr/libexec/arka-rtc-mode /usr/libexec/arka-dns-after-clock /usr/bin/firefox-sandbox /usr/bin/arka-convert && \
    mkdir -p /usr/lib/systemd/system/bootc-fetch-apply-updates.service.d && \
    printf '[Service]\nExecStart=\nExecStart=/usr/bin/bootc upgrade --quiet\n' \
      > /usr/lib/systemd/system/bootc-fetch-apply-updates.service.d/10-arkaos-no-auto-reboot.conf && \
    cp -a /usr/share/sddm/themes/arkaos /usr/share/sddm/themes/arkaos-light && \
    sed -i -e 's/^look=.*/look=light/' \
           -e 's|^background=.*|background=/usr/share/arka/wallpapers/default-light.png|' \
           /usr/share/sddm/themes/arkaos-light/theme.conf && \
    sed -i -e 's/^Name=.*/Name=ArkaOS Light/' -e 's/^Theme-Id=.*/Theme-Id=arkaos-light/' \
           /usr/share/sddm/themes/arkaos-light/metadata.desktop && \
    mv /usr/bin/firefox /usr/bin/firefox-unwrapped && \
    ln -sf firefox-sandbox /usr/bin/firefox && \
    for v in dark light; do \
      d=/usr/share/plasma/desktoptheme/arkaos-$v && \
      if [ $v = dark ]; then n=Dark; else n=Light; fi && \
      cp /usr/share/plasma/desktoptheme/breeze-$v/colors /usr/share/plasma/desktoptheme/breeze-$v/plasmarc $d/ && \
      printf '{"KPlugin": {"Id": "arkaos-%s", "Name": "ArkaOS %s", "Description": "Breeze %s with ArkaOS panels", "License": "LGPL-2.1-or-later"}, "X-Plasma-API": "5.0"}\n' \
        $v $n $n > $d/metadata.json || exit 1; \
    done && \
    printf '\n# ArkaOS: also ask these time servers by address. With a dead clock battery the date\n# starts wrong, DNS then rejects every answer, and a time service that needs DNS\n# can never fix the date. Cloudflare and Google answer on fixed addresses.\nserver 162.159.200.1 iburst\nserver 162.159.200.123 iburst\nserver 216.239.35.0 iburst\nserver 216.239.35.4 iburst\n' >> /etc/chrony.conf && \
    mkdir -p /usr/share/arkaos/arkasetup && \
    cp -a /var/home/arkasetup/.bash_profile /var/home/arkasetup/.hushlogin /usr/share/arkaos/arkasetup/ && \
    systemctl enable arkad.service arka-pulsed.service arka-rtc-mode.service arka-dns-after-clock.service

# Built binaries go in LAST: they change on every Rust edit, and anything after
# a changed layer rebuilds. Units and configs that reference them are installed
# (and enabled) above; nothing in between executes them. arka-bar / arka-dock /
# arka-launcher are not shipped: Plasma's panel, Crystal Dock and kickerdash
# replace them (the old ones are in git history, before 0d707ca).
COPY --from=bins / /
RUN chmod 755 /usr/bin/arkad /usr/bin/arka-pulsed /usr/bin/arka-oobe \
      /usr/bin/arka-dashboard /usr/bin/arka-wifi /usr/bin/arka-update \
      /usr/bin/arka-hotkeys /usr/bin/arka-capsule /usr/bin/arka-perms \
      /usr/bin/arka-settings-gtk /usr/bin/arka-welcome /usr/bin/arka-sound \
      /usr/bin/arka-bluetooth /usr/bin/arka-privacy-tray /usr/bin/arka-privacy-dots && \
    bootc container lint
