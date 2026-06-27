# Stage 1: build arkad as a static musl binary
FROM docker.io/rust:alpine AS builder
RUN apk add --no-cache musl-dev
COPY arkad/ /build/
COPY arka-shell/arka-shell-common/ /arka-shell/arka-shell-common/
WORKDIR /build
RUN cargo build --release --target x86_64-unknown-linux-musl

# Stage 2: build arka-bar (GTK4 layer-shell bar — glibc required, not musl)
FROM docker.io/fedora:42 AS shell-builder
RUN dnf install -y -q gtk4-devel gtk4-layer-shell-devel libadwaita-devel rust cargo gcc pkgconf-pkg-config
COPY arka-shell/ /build/
WORKDIR /build
RUN cargo build --release

# Stage 3: bootc image
FROM quay.io/fedora/fedora-bootc:42

RUN echo "arkaos-dev" > /etc/arkaos-release && \
    printf 'NAME="ArkaOS"\nPRETTY_NAME="ArkaOS 0.1"\nID=arkaos\nID_LIKE=fedora\nVERSION="0.1"\nVERSION_ID="0.1"\nHOME_URL="https://github.com/thulasiramk-2310/Arka"\nBUG_REPORT_URL="https://github.com/thulasiramk-2310/Arka/issues"\n' \
      > /etc/os-release

RUN mkdir -p /etc/NetworkManager/conf.d && \
    printf '[device]\nwifi.scan-rand-mac-address=yes\n\n[connection]\nwifi.cloned-mac-address=random\nethernet.cloned-mac-address=random\n' \
      > /etc/NetworkManager/conf.d/00-arkaos-mac-random.conf

RUN mkdir -p /usr/lib/bootc/install && \
    printf '[install.filesystem.root]\ntype = "xfs"\n' \
      > /usr/lib/bootc/install/00-defaults.toml

# Phase 3a: UKI toolchain + systemd-boot EFI binary
RUN dnf install -y systemd-ukify systemd-boot-unsigned binutils

# Switch kernel-install to UKI layout
RUN printf 'layout=uki\nuki_generator=ukify\n' > /etc/kernel/install.conf

# Pre-build UKI from installed kernel+initramfs — no embedded cmdline
RUN set -e; \
    KVER=$(ls /usr/lib/modules | head -1); \
    ukify build \
      --linux  /usr/lib/modules/${KVER}/vmlinuz \
      --initrd /usr/lib/modules/${KVER}/initramfs.img \
      --stub   /usr/lib/systemd/boot/efi/linuxx64.efi.stub \
      --os-release /etc/os-release \
      --output /usr/lib/modules/${KVER}/${KVER}.efi; \
    echo "UKI built: $(ls -lh /usr/lib/modules/${KVER}/${KVER}.efi)"

# Install arkad
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/arkad /usr/bin/arkad
COPY arkad/arkad.service /usr/lib/systemd/system/arkad.service
COPY arkad/arkad.toml /etc/arkad/arkad.toml
COPY arkad/org.arka.arkad.conf /etc/dbus-1/system.d/org.arka.arkad.conf
COPY arkad/org.arka.arkad.service /usr/share/dbus-1/system-services/org.arka.arkad.service
RUN chmod 755 /usr/bin/arkad && \
    mkdir -p /etc/arkad && \
    systemctl enable arkad.service

# Phase 4: bubblewrap-sandboxed Firefox
RUN dnf install -y firefox bubblewrap
COPY arkaos-firefox /usr/bin/firefox-sandbox
RUN chmod 755 /usr/bin/firefox-sandbox && \
    mv /usr/bin/firefox /usr/bin/firefox-unwrapped && \
    ln -sf firefox-sandbox /usr/bin/firefox

# Graphical session: full KDE Plasma spin + SDDM login manager.
# Plasma provides the panel, launcher, settings, file manager, window
# management and login — replacing the custom Hyprland shell. The ArkaOS
# privacy apps (dashboard, capsule, settings) run as regular Plasma apps.
# rootfiles is excluded: it ships /root dotfiles that already exist in the
# bootc base, so its cpio unpack fails the whole transaction. It's cosmetic.
RUN dnf install -y --exclude=rootfiles @kde-desktop-environment sddm \
    pipewire wireplumber pipewire-pulseaudio xorg-x11-server-Xwayland \
    libadwaita gtk4-layer-shell flatpak xdg-user-dirs brightnessctl \
    bluez bluez-tools && \
    dnf clean all

# Enable SDDM (started once firstboot flips the default to graphical.target).
# Default to the Plasma Wayland session.
RUN systemctl enable sddm.service && \
    mkdir -p /etc/sddm.conf.d && \
    printf '[General]\nDisplayServer=wayland\n\n[Wayland]\nSessionDir=/usr/share/wayland-sessions\n' \
      > /etc/sddm.conf.d/10-wayland.conf


# Install arka-bar (replaces waybar)
COPY --from=shell-builder /build/target/release/arka-bar /usr/bin/arka-bar
RUN chmod 755 /usr/bin/arka-bar

# Install arka-shell binaries
COPY --from=shell-builder /build/target/release/arka-dashboard /usr/bin/arka-dashboard
COPY --from=shell-builder /build/target/release/arka-launcher  /usr/bin/arka-launcher
COPY --from=shell-builder /build/target/release/arka-wifi      /usr/bin/arka-wifi
COPY --from=shell-builder /build/target/release/arka-update    /usr/bin/arka-update
COPY --from=shell-builder /build/target/release/arka-hotkeys   /usr/bin/arka-hotkeys
COPY --from=shell-builder /build/target/release/arka-capsule   /usr/bin/arka-capsule
COPY --from=shell-builder /build/target/release/arka-perms    /usr/bin/arka-perms
COPY --from=shell-builder /build/target/release/arka-settings /usr/bin/arka-settings-gtk
COPY --from=shell-builder /build/target/release/arka-welcome  /usr/bin/arka-welcome
COPY --from=shell-builder /build/target/release/arka-sound      /usr/bin/arka-sound
COPY --from=shell-builder /build/target/release/arka-bluetooth  /usr/bin/arka-bluetooth
COPY --from=shell-builder /build/target/release/arka-dock       /usr/bin/arka-dock
RUN chmod 755 /usr/bin/arka-dashboard /usr/bin/arka-launcher /usr/bin/arka-wifi \
              /usr/bin/arka-update /usr/bin/arka-hotkeys /usr/bin/arka-capsule \
              /usr/bin/arka-perms /usr/bin/arka-settings-gtk /usr/bin/arka-welcome \
              /usr/bin/arka-sound /usr/bin/arka-bluetooth /usr/bin/arka-dock

# mako notification config + skel/Pictures for screenshots
RUN mkdir -p /etc/skel/.config/mako /etc/skel/Pictures && \
    printf '[global]\nbackground-color=#0d0d1aff\ntext-color=#d0dff0ff\nborder-color=#1a3a5aff\nborder-radius=8\nborder-size=1\nfont=Liberation Sans 12\nwidth=320\nmargin=10\npadding=12\ndefault-timeout=4000\n' \
      > /etc/skel/.config/mako/config

# Disable PAM password quality enforcement — firstboot wizard handles its own validation
RUN mkdir -p /etc/security/pwquality.conf.d && \
    printf 'minlen=1\ndcredit=0\nucredit=0\nlcredit=0\nocredit=0\nminclass=0\n' \
      > /etc/security/pwquality.conf.d/00-arkaos.conf

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
    systemctl enable arkaos-firstboot.service && \
    echo '%wheel ALL=(ALL) NOPASSWD: /usr/bin/arkaos-settings' \
      > /etc/sudoers.d/99-arkaos-settings

# ArkaOS signature wallpaper — deep black + blue glow + triangle grid + identity mark
RUN dnf install -y -q ImageMagick && \
    mkdir -p /usr/share/arka/wallpapers && \
    magick -size 1920x1080 xc:"#07080e" \
      \( -size 1920x1080 radial-gradient:"#0d3060-#07080e" -sigmoidal-contrast 4,50% \) \
      -compose Screen -composite \
      \( -size 1920x1080 radial-gradient:"#051828-#07080e" \
         -distort SRT "960,680 1 0 960,680" \) \
      -compose Multiply -composite \
      \( -size 40x40 xc:none \
         -fill "rgba(30,90,160,0.18)" -draw "point 0,0" \
         -fill "rgba(30,90,160,0.10)" -draw "point 20,0" \
         -fill "rgba(30,90,160,0.10)" -draw "point 0,20" \
         -fill "rgba(30,90,160,0.06)" -draw "point 20,20" \
         -write mpr:grid +delete \
         -size 1920x1080 tile:mpr:grid \) \
      -compose Over -composite \
      \( -size 1920x1080 xc:none \
         -fill "rgba(0,160,255,0.04)" \
         -draw "polygon 960,300 860,480 1060,480" \
         -fill "rgba(0,160,255,0.025)" \
         -draw "polygon 960,220 810,490 1110,490" \
         -fill "rgba(0,160,255,0.015)" \
         -draw "polygon 960,140 760,500 1160,500" \) \
      -compose Over -composite \
      -font /usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf \
      -fill "rgba(30,210,140,0.90)" \
      -gravity Center -pointsize 92 -draw "text 0,-52 'ARKAOS'" \
      -fill "rgba(30,210,140,0.35)" \
      -gravity Center -pointsize 92 -draw "text 2,-50 'ARKAOS'" \
      -fill "rgba(200,220,245,0.75)" \
      -font /usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf \
      -pointsize 18 -kerning 5 \
      -gravity Center -draw "text 0,56 'YOUR COMPUTER IS YOURS'" \
      -fill "rgba(30,90,160,0.40)" \
      -gravity Center -pointsize 18 -kerning 5 \
      -draw "text 0,86 'privacy  ·  security  ·  freedom'" \
      /usr/share/arka/wallpapers/default.png && \
    dnf remove -y -q ImageMagick

# arka-tools .desktop files for launcher search
COPY desktop-files/ /usr/share/applications/

# KDE/SDDM owns the graphical session — no tty autostart, no Hyprland config.

RUN bootc container lint
