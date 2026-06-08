# Stage 1: build arkad as a static musl binary
FROM docker.io/rust:alpine AS builder
RUN apk add --no-cache musl-dev
COPY arkad/ /build/
WORKDIR /build
RUN cargo build --release --target x86_64-unknown-linux-musl

# Stage 2: bootc image
FROM quay.io/fedora/fedora-bootc:42

RUN echo "arkaos-dev" > /etc/arkaos-release && \
    printf 'NAME="ArkaOS"\nPRETTY_NAME="ArkaOS 0.1 — by Thulasi Ram K"\nID=arkaos\nID_LIKE=fedora\nVERSION="0.1"\nVERSION_ID="0.1"\nHOME_URL="https://github.com/thulasiramk-2310/Arka"\nBUG_REPORT_URL="https://github.com/thulasiramk-2310/Arka/issues"\n' \
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

# Graphical session: sway Wayland compositor
RUN dnf install -y sway foot xorg-x11-server-Xwayland pipewire wireplumber \
    pipewire-pulseaudio

# Autologin ram on tty1
RUN mkdir -p /etc/systemd/system/getty@tty1.service.d && \
    printf '[Service]\nExecStart=\nExecStart=-/sbin/agetty --autologin ram --noclear %%I $TERM\n' \
      > /etc/systemd/system/getty@tty1.service.d/autologin.conf

# Sway config + autostart via /etc/skel (copied to home on first login)
RUN mkdir -p /etc/skel/.config/sway
COPY arkaos-sway-config /etc/skel/.config/sway/config
COPY sway-autostart     /etc/skel/.bash_profile

RUN bootc container lint
