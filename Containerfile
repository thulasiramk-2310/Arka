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
# retries=25 + fastestmirror ride out the transient Fedora mirror hiccups that
# otherwise fail this 1500-package transaction ("All mirrors were tried").
RUN dnf install -y --setopt=retries=25 --setopt=fastestmirror=True \
    --setopt=max_parallel_downloads=4 --exclude=rootfiles \
    @kde-desktop-environment sddm \
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


# Install arkad
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/arkad /usr/bin/arkad
COPY arkad/arkad.service /usr/lib/systemd/system/arkad.service
COPY arkad/arkad.toml /etc/arkad/arkad.toml
COPY arkad/org.arka.arkad.conf /etc/dbus-1/system.d/org.arka.arkad.conf
COPY arkad/org.arka.arkad.service /usr/share/dbus-1/system-services/org.arka.arkad.service
RUN chmod 755 /usr/bin/arkad && \
    mkdir -p /etc/arkad && \
    systemctl enable arkad.service

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
      -fill "rgba(22,199,132,0.90)" \
      -gravity Center -pointsize 92 -draw "text 0,-52 'ARKAOS'" \
      -fill "rgba(22,199,132,0.35)" \
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

# ── ArkaWM: brand KDE Plasma as ArkaOS ──────────────────────────────────────
# Inter typeface (ArkaOS design system) + the ArkaOS Plasma color scheme.
RUN dnf install -y rsms-inter-fonts && dnf clean all
COPY ArkaOS.colors /usr/share/color-schemes/ArkaOS.colors

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

# Custom ArkaOS SDDM login theme (dark, green accent, fade-in animation).
COPY sddm-theme-arkaos/ /usr/share/sddm/themes/arkaos/
RUN printf '[Theme]\nCurrent=arkaos\n' > /etc/sddm.conf.d/30-arka-theme.conf

# Lock screen: ArkaOS wallpaper behind the (already dark+green) Plasma locker.
RUN printf '[Greeter][Wallpaper][org.kde.image][General]\nImage=/usr/share/arka/wallpapers/default.png\n' \
      > /etc/skel/.config/kscreenlockerrc

# KWin desktop effects for a more interactive feel: wobbly windows, magic-lamp
# minimise, and scale on window open/close.
RUN printf '[Plugins]\nwobblywindowsEnabled=true\nmagiclampEnabled=true\nkwin4_effect_scaleEnabled=true\n' \
      > /etc/skel/.config/kwinrc

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
RUN dnf install -y --setopt=retries=25 plymouth-plugin-script && \
    dnf clean all

# ArkaCursor: white pointer with green outline, generated at build; every
# other shape inherits from breeze_cursors. ArkaSound: soft sine-based event
# sounds. Plymouth splash images. All need ImageMagick/xcursorgen/sox once.
COPY plymouth-theme-arkaos/ /usr/share/plymouth/themes/arkaos/
RUN dnf install -y -q ImageMagick xcursorgen sox && \
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
    magick -size 260x230 xc:none -fill none -stroke "#16c784" -strokewidth 5 \
      -draw "polygon 130,35 205,175 55,175" tri1.png && \
    magick -size 260x230 xc:none -fill none -stroke "#16c784" -strokewidth 3 \
      -draw "polygon 130,12 228,196 32,196" tri2.png && \
    magick -size 260x230 xc:none -fill none -stroke "#16c784" -strokewidth 8 \
      -draw "polygon 130,12 228,196 32,196" -blur 0x6 tri3.png && \
    magick -size 400x70 xc:none \
      -font /usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf \
      -fill "#16c784" -pointsize 44 -kerning 14 -gravity center \
      -draw "text 0,0 'ARKA'" wordmark.png && \
    magick -size 8x3 xc:"#16c784" bar.png && \
    magick -size 8x3 xc:"#1e2630" barback.png && \
    rm -rf /tmp/cur && \
    dnf remove -y -q ImageMagick xcursorgen sox && dnf clean all

# Default cursor theme for new users.
RUN printf '[Mouse]\ncursorTheme=ArkaCursor\n' > /etc/skel/.config/kcminputrc

# Activate the boot splash: set the theme, rebuild the initramfs so plymouth
# carries it, and add rhgb+quiet kargs so the splash actually shows.
RUN plymouth-set-default-theme arkaos && \
    KVER=$(ls /usr/lib/modules | head -1) && \
    dracut --force --no-hostonly /usr/lib/modules/$KVER/initramfs.img $KVER && \
    mkdir -p /usr/lib/bootc/kargs.d && \
    printf 'kargs = ["rhgb", "quiet"]\n' > /usr/lib/bootc/kargs.d/10-arkaos-splash.toml

RUN bootc container lint

