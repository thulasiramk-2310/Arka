# Stage 1: build arkad as a static musl binary
FROM docker.io/rust:alpine AS builder
RUN apk add --no-cache musl-dev
COPY arkad/ /build/
WORKDIR /build
RUN cargo build --release --target x86_64-unknown-linux-musl

# Stage 2: bootc image
FROM quay.io/centos-bootc/centos-bootc:stream10

RUN echo "arkaos-dev" > /etc/arkaos-release

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
# Fix 1: interception at build time — wrapper replaces /usr/bin/firefox in the
#         image layer; composefs makes it read-only at runtime, so no escape hatch.
# Fix 3: wrapper uses /etc allowlist (see arkaos-firefox), not full /etc bind.
RUN dnf install -y firefox bubblewrap
COPY arkaos-firefox /usr/bin/firefox-sandbox
RUN chmod 755 /usr/bin/firefox-sandbox && \
    mv /usr/bin/firefox /usr/bin/firefox-unwrapped && \
    ln -sf firefox-sandbox /usr/bin/firefox

RUN bootc container lint
