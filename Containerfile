# Stage 1: build arkad as a static musl binary
FROM docker.io/rust:alpine AS builder
RUN apk add --no-cache musl-dev
COPY arkad/ /build/
WORKDIR /build
RUN cargo build --release --target x86_64-unknown-linux-musl

# Stage 2: bootc image
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
RUN chmod 755 /usr/bin/arkad && \
    mkdir -p /etc/arkad && \
    systemctl enable arkad.service

# Phase 4: bubblewrap-sandboxed Firefox
RUN dnf install -y firefox bubblewrap
COPY arkaos-firefox /usr/bin/firefox-sandbox
RUN chmod 755 /usr/bin/firefox-sandbox && \
    mv /usr/bin/firefox /usr/bin/firefox-unwrapped && \
    ln -sf firefox-sandbox /usr/bin/firefox

# Graphical session: Hyprland + launcher + file manager
RUN dnf install -y hyprland waybar swaybg foot xorg-x11-server-Xwayland \
    pipewire wireplumber pipewire-pulseaudio \
    wofi thunar

# Disable PAM password quality enforcement — firstboot wizard handles its own validation
RUN mkdir -p /etc/security/pwquality.conf.d && \
    printf 'minlen=1\ndcredit=0\nucredit=0\nlcredit=0\nocredit=0\nminclass=0\n' \
      > /etc/security/pwquality.conf.d/00-arkaos.conf

# First-boot setup wizard + settings utility
COPY arkaos-firstboot         /usr/libexec/arkaos-firstboot
COPY arkaos-firstboot.service /usr/lib/systemd/system/arkaos-firstboot.service
COPY arkaos-settings          /usr/bin/arkaos-settings
RUN chmod 755 /usr/libexec/arkaos-firstboot /usr/bin/arkaos-settings && \
    systemctl enable arkaos-firstboot.service && \
    echo '%wheel ALL=(ALL) NOPASSWD: /usr/bin/arkaos-settings' \
      > /etc/sudoers.d/99-arkaos-settings

# Hyprland config + waybar + autostart via /etc/skel
RUN mkdir -p /etc/skel/.config/hypr /etc/skel/.config/waybar
COPY arkaos-hyprland-config    /etc/skel/.config/hypr/hyprland.conf
COPY arkaos-waybar-config      /etc/skel/.config/waybar/config.jsonc
COPY arkaos-waybar-style       /etc/skel/.config/waybar/style.css
COPY sway-autostart            /etc/skel/.bash_profile

RUN bootc container lint
