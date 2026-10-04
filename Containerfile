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
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked dnf install --setopt=keepcache=True -y -q gtk4-devel libadwaita-devel rust cargo gcc pkgconf-pkg-config
COPY arka-shell/ /build/
WORKDIR /build
RUN cargo build --locked --release

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
COPY arkaos-oobe-apply /usr/libexec/arkaos-oobe-apply
RUN chmod 755 /usr/libexec/arkaos-oobe-apply && \
    useradd -m -s /bin/bash arkasetup && passwd -l arkasetup && \
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
RUN mkdir -p /etc/skel/.config/autostart /usr/share/arka
COPY arka-layout.js /usr/share/arka/arka-layout.js
COPY arka-plasma-firstrun /usr/libexec/arka-plasma-firstrun
RUN chmod 755 /usr/libexec/arka-plasma-firstrun && \
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

# Desktop wallpaper preset so the Arka wallpaper shows from the FIRST frame
# (kills the default-Plasma → Arka flash). arka-plasma-firstrun still applies it
# via plasma-apply-wallpaperimage as a fallback if the containment id differs.
RUN printf '[Containments][1]\nwallpaperplugin=org.kde.image\n\n[Containments][1][Wallpaper][org.kde.image][General]\nImage=file:///usr/share/arka/wallpapers/default.png\n' \
      > /etc/skel/.config/plasma-org.kde.plasma.desktop-appletsrc

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

# Light/Dark switcher shared by first login and ArkaOS Settings > Appearance.
COPY arka-apply-look /usr/libexec/arka-apply-look

# Light login screen: the same theme with look=light and the light wallpaper.
# arkaos-oobe-apply selects it when the user picks Light in setup.
RUN cp -a /usr/share/sddm/themes/arkaos /usr/share/sddm/themes/arkaos-light && \
    sed -i -e 's/^look=.*/look=light/' \
           -e 's|^background=.*|background=/usr/share/arka/wallpapers/default-light.png|' \
           /usr/share/sddm/themes/arkaos-light/theme.conf && \
    sed -i -e 's/^Name=.*/Name=ArkaOS Light/' -e 's/^Theme-Id=.*/Theme-Id=arkaos-light/' \
           /usr/share/sddm/themes/arkaos-light/metadata.desktop

# arkad's polkit action (admin auth for EnforceAll). Placed late: a new file
# early in this stage would invalidate the cached package layers below it.
COPY arkad/org.arka.arkad.policy /usr/share/polkit-1/actions/org.arka.arkad.policy

# Small, often-edited files live here, AFTER every package install: changing
# one of them must only rerun these cheap steps, never the 1500-package KDE
# install (moving them here cost one full rebuild; keeping them early cost one
# per edit).
# bubblewrap-sandboxed Firefox wrapper (firefox itself is installed above)
COPY arkaos-firefox /usr/bin/firefox-sandbox
RUN chmod 755 /usr/bin/firefox-sandbox && \
    mv /usr/bin/firefox /usr/bin/firefox-unwrapped && \
    ln -sf firefox-sandbox /usr/bin/firefox

# Install arkad
COPY arkad/arkad.service /usr/lib/systemd/system/arkad.service
COPY arkad/arkad.toml /etc/arkad/arkad.toml
COPY arkad/org.arka.arkad.conf /etc/dbus-1/system.d/org.arka.arkad.conf
COPY arkad/org.arka.arkad.service /usr/share/dbus-1/system-services/org.arka.arkad.service
RUN mkdir -p /etc/arkad && \
    systemctl enable arkad.service

# Install arka-pulsed (reliability daemon; read-only org.arka.pulse, mirrors arkad)
COPY arka-pulse/arka-pulsed.service /usr/lib/systemd/system/arka-pulsed.service
COPY arka-pulse/org.arka.pulse.conf /etc/dbus-1/system.d/org.arka.pulse.conf
COPY arka-pulse/org.arka.pulse.service /usr/share/dbus-1/system-services/org.arka.pulse.service
COPY docs/RELIABILITY-ARKA-PULSE.md /usr/share/doc/arkaos/RELIABILITY-ARKA-PULSE.md
RUN systemctl enable arka-pulsed.service

# Firefox managed privacy policy (see the comment near the KDE install).
COPY firefox-policies.json /usr/lib64/firefox/distribution/policies.json

# Built binaries go in LAST: they change on every Rust edit, and anything after
# a changed layer rebuilds. Copied here, a code change only reruns this block
# instead of the themes, wallpaper, splash and initramfs above. Units and
# configs that reference them are installed (and enabled) earlier; nothing in
# between executes them. arka-bar / arka-dock / arka-launcher are not shipped:
# Plasma's panel, icontasks dock and kickerdash replace them (see legacy/).
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
RUN chmod 755 /usr/bin/arkad /usr/bin/arka-pulsed /usr/bin/arka-oobe \
      /usr/bin/arka-dashboard /usr/bin/arka-wifi /usr/bin/arka-update \
      /usr/bin/arka-hotkeys /usr/bin/arka-capsule /usr/bin/arka-perms \
      /usr/bin/arka-settings-gtk /usr/bin/arka-welcome /usr/bin/arka-sound \
      /usr/bin/arka-bluetooth /usr/bin/arka-privacy-tray

RUN bootc container lint
