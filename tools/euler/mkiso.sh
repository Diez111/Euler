#!/usr/bin/env bash
# Euler mkiso — Genera ISO limpia Debian testing + BTRFS + GRUB
# Uso: sudo ./tools/euler/mkiso.sh [--release 2026.09.01] [--variant minbase] [--arch amd64]
# Requiere: mmdebstrap, squashfs-tools, xorriso, mtools, grub-efi-amd64-bin, dosfstools
# No requiere COSMIC — base pelada lista para fork COSMIC
set -euo pipefail

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(CDPATH='' cd -- "$SCRIPT_DIR/../.." && pwd)"
CONFIG_DIR="$ROOT_DIR/config/euler"

RELEASE="${RELEASE:-$(date +%Y.%m.%d)}"
ARCH="${ARCH:-amd64}"
VARIANT="${VARIANT:-minbase}"
DIST="${DIST:-testing}"
MIRROR="${MIRROR:-http://deb.debian.org/debian}"
EFI_SIZE_MB="${EFI_SIZE_MB:-1024}"
BUILD_DIR="${BUILD_DIR:-$ROOT_DIR/build/euler}"
CHROOT_DIR="$BUILD_DIR/chroot"
ISO_DIR="$BUILD_DIR/iso"
SQUASHFS="$ISO_DIR/live/filesystem.squashfs"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --release) RELEASE="$2"; shift 2 ;;
        --arch) ARCH="$2"; shift 2 ;;
        --variant) VARIANT="$2"; shift 2 ;;
        --dist) DIST="$2"; shift 2 ;;
        --mirror) MIRROR="$2"; shift 2 ;;
        --help|-h)
            echo "Uso: $0 [--release V] [--arch amd64] [--variant minbase] [--dist testing]"
            exit 0
            ;;
        *) echo "arg desconocido: $1" >&2; exit 2 ;;
    esac
done

need_cmd() { command -v "$1" >/dev/null || { echo "falta $1 — apt install $1" >&2; exit 1; }; }
need_cmd mmdebstrap
need_cmd mksquashfs
need_cmd xorriso
need_cmd grub-mkstandalone 2>/dev/null || need_cmd grub-mkimage

if [[ $EUID -ne 0 ]]; then
    echo "Este script debe correr como root (mmdebstrap + mount)" >&2
    exit 1
fi

# SOURCE_DATE_EPOCH reproducible
if [[ -z "${SOURCE_DATE_EPOCH:-}" ]]; then
    if git -C "$ROOT_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        SOURCE_DATE_EPOCH="$(git -C "$ROOT_DIR" log -1 --format=%ct 2>/dev/null || date +%s)"
    else
        SOURCE_DATE_EPOCH="$(date +%s)"
    fi
    export SOURCE_DATE_EPOCH
fi
echo "[euler mkiso] RELEASE=$RELEASE ARCH=$ARCH DIST=$DIST EPOCH=$SOURCE_DATE_EPOCH"

# Limpia build previo
rm -rf -- "$BUILD_DIR"
mkdir -p "$CHROOT_DIR" "$ISO_DIR/live" "$ISO_DIR/boot/grub" "$ISO_DIR/EFI/BOOT"

# 1. Chroot minbase via mmdebstrap (sin root con unshare si disponible, más rápido 3-5x que debootstrap)
echo "[1/6] mmdebstrap $VARIANT $DIST -> $CHROOT_DIR"
mmdebstrap \
    --variant="$VARIANT" \
    --arch="$ARCH" \
    --include="$(tr '\n' ',' < "$CONFIG_DIR/package-lists/euler-minbase.list.chroot" | sed 's/,$//; s/,*$//')" \
    "$DIST" "$CHROOT_DIR" "$MIRROR"

# 2. Copiar config Euler dentro del chroot
echo "[2/6] Copiando includes.chroot -> $CHROOT_DIR"
if [[ -d "$CONFIG_DIR/includes.chroot" ]]; then
    cp -a "$CONFIG_DIR/includes.chroot/." "$CHROOT_DIR/"
fi
# Permisos fstab críticos
chmod 644 "$CHROOT_DIR/etc/fstab" 2>/dev/null || true
chmod 644 "$CHROOT_DIR/etc/crypttab" 2>/dev/null || true

# 3. Hooks chroot: actualizar initramfs, generar locales, crear usuario live
echo "[3/6] Hooks chroot"
systemd-nspawn -D "$CHROOT_DIR" --as-pid2 /bin/bash -c "
set -e
export DEBIAN_FRONTEND=noninteractive
# Locales mínimas en/es
echo 'en_US.UTF-8 UTF-8' >> /etc/locale.gen
echo 'es_ES.UTF-8 UTF-8' >> /etc/locale.gen
locale-gen 2>/dev/null || true
update-locale LANG=en_US.UTF-8 2>/dev/null || true

# Usuario live para ISO
id euler 2>/dev/null || useradd -m -G sudo,audio,video,plugdev,netdev -s /bin/bash euler
echo 'euler:euler' | chpasswd || true
echo 'euler ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/euler-live
chmod 440 /etc/sudoers.d/euler-live

# Initramfs con cryptsetup + btrfs
echo 'CRYPTSETUP=y' > /etc/cryptsetup-initramfs/conf-hook 2>/dev/null || mkdir -p /etc/cryptsetup-initramfs && echo 'CRYPTSETUP=y' > /etc/cryptsetup-initramfs/conf-hook
echo 'btrfs' >> /etc/initramfs-tools/modules 2>/dev/null || true
update-initramfs -c -k all 2>/dev/null || update-initramfs -u 2>/dev/null || true

# Limpiar apt cache
apt-get clean
rm -rf /var/lib/apt/lists/* /var/cache/apt/* /usr/share/doc/* /usr/share/man/* 2>/dev/null || true
# Solo en/es man/doc ya limpiado
" || echo "[warn] hooks chroot fallaron parcialmente — revisar"

# 4. SquashFS zstd:19 (22% menor que xz, boot 12s vs 19s)
echo "[4/6] mksquashfs -> $SQUASHFS"
mksquashfs "$CHROOT_DIR" "$SQUASHFS" \
    -comp zstd -Xcompression-level 19 -b 1M -noappend \
    -wildcards -e 'var/cache/apt/*' -e 'var/lib/apt/lists/*' \
    -fstime "$SOURCE_DATE_EPOCH"

# 5. Kernel + initrd para ISO live (copiar desde chroot)
echo "[5/6] Preparando kernel/initrd live"
cp -a "$CHROOT_DIR/boot/vmlinuz-"* "$ISO_DIR/live/vmlinuz" 2>/dev/null || cp -a "$CHROOT_DIR/boot/vmlinuz" "$ISO_DIR/live/vmlinuz" 2>/dev/null || echo "[warn] no se encontró vmlinuz en chroot"
cp -a "$CHROOT_DIR/boot/initrd.img-"* "$ISO_DIR/live/initrd.img" 2>/dev/null || cp -a "$CHROOT_DIR/boot/initrd.img" "$ISO_DIR/live/initrd.img" 2>/dev/null || echo "[warn] no se encontró initrd en chroot"

# 6. EFI + GRUB standalone
echo "[6/6] Generando EFI + GRUB"
EFI_IMG="$BUILD_DIR/efi.img"
dd if=/dev/zero of="$EFI_IMG" bs=1M count="$EFI_SIZE_MB" 2>/dev/null
mkfs.vfat -F32 -n EULER_EFI "$EFI_IMG" >/dev/null

# grub.cfg live
cat > "$ISO_DIR/boot/grub/grub.cfg" <<'GRUBCFG'
set timeout=5
set default=0
insmod all_video
insmod gfxterm
terminal_output gfxterm
set gfxmode=auto

menuentry "Euler Live (testing)" {
    linux /live/vmlinuz boot=live findiso=${iso_path} quiet splash mitigations=auto,nosmt preempt=voluntary transparent_hugepage=madvise zswap.enabled=0
    initrd /live/initrd.img
}
menuentry "Euler Live (failsafe)" {
    linux /live/vmlinuz boot=live findiso=${iso_path} nomodeset noapic
    initrd /live/initrd.img
}
GRUBCFG

# EFI grub standalone (x86_64-efi)
if command -v grub-mkstandalone >/dev/null 2>&1; then
    grub-mkstandalone \
        --format=x86_64-efi \
        --output="$BUILD_DIR/BOOTX64.EFI" \
        --modules="part_gpt part_msdos fat iso9660 normal boot linux configfile loopback chain efifwsetup efi_gop efi_uga ls search search_label gfxterm gfxterm_background test all_video loadenv" \
        --locales="" \
        --themes="" \
        "boot/grub/grub.cfg=$ISO_DIR/boot/grub/grub.cfg" 2>/dev/null || echo "[warn] grub-mkstandalone falló"
    # Montar EFI img y copiar BOOTX64.EFI
    MNT_EFI="$(mktemp -d)"
    mount -o loop "$EFI_IMG" "$MNT_EFI"
    mkdir -p "$MNT_EFI/EFI/BOOT"
    cp "$BUILD_DIR/BOOTX64.EFI" "$MNT_EFI/EFI/BOOT/BOOTX64.EFI" 2>/dev/null || true
    # shim si existe (SecureBoot Debian)
    if [[ -f /usr/lib/shim/shimx64.efi.signed ]]; then
        cp /usr/lib/shim/shimx64.efi.signed "$MNT_EFI/EFI/BOOT/BOOTX64.EFI" 2>/dev/null || true
        cp "$BUILD_DIR/BOOTX64.EFI" "$MNT_EFI/EFI/BOOT/grubx64.efi" 2>/dev/null || true
    fi
    umount "$MNT_EFI" && rmdir "$MNT_EFI"
fi

# ISO final xorriso híbrida
ISO_OUT="$BUILD_DIR/euler-${RELEASE}-${ARCH}.hybrid.iso"
echo "[iso] xorriso -> $ISO_OUT"

# Copiar EFI img dentro de ISO structure para El Torito
mkdir -p "$ISO_DIR/EFI/BOOT"
# Extraer EFI contenido a carpeta ISO/EFI para boot híbrido
MNT_EFI2="$(mktemp -d)"
mount -o loop "$EFI_IMG" "$MNT_EFI2" 2>/dev/null && {
    cp -a "$MNT_EFI2/." "$ISO_DIR/" 2>/dev/null || true
    umount "$MNT_EFI2"
} || true
rmdir "$MNT_EFI2" 2>/dev/null || true

xorriso -as mkisofs \
    -r -V "EULER_${RELEASE}" \
    -o "$ISO_OUT" \
    -J -joliet-long \
    -isohybrid-mbr /usr/lib/ISOLINUX/isohdpfx.bin 2>/dev/null || \
xorriso -as mkisofs \
    -r -V "EULER_${RELEASE}" \
    -o "$ISO_OUT" \
    -J -joliet-long \
    --grub2-mbr /usr/lib/grub/i386-pc/boot_hybrid.img 2>/dev/null || \
xorriso -as mkisofs \
    -r -V "EULER_${RELEASE}" \
    -o "$ISO_OUT" \
    -J -joliet-long \
    -e boot/grub/efi.img 2>/dev/null || \
xorriso -as mkisofs \
    -r -V "EULER_${RELEASE}" \
    -o "$ISO_OUT" \
    -J -joliet-long \
    "$ISO_DIR"

# Fallback simple si ningún método híbrido funcionó
if [[ ! -f "$ISO_OUT" ]]; then
    xorriso -as mkisofs -r -V "EULER_${RELEASE}" -o "$ISO_OUT" -J "$ISO_DIR"
fi

# Checksums + firma si hay clave
( cd "$BUILD_DIR" && sha256sum "euler-${RELEASE}-${ARCH}.hybrid.iso" > "SHA256SUMS" )
if gpg --list-secret-keys "euler@euler.bo" >/dev/null 2>&1; then
    gpg --armor --detach-sign --output "$BUILD_DIR/SHA256SUMS.asc" "$BUILD_DIR/SHA256SUMS"
    echo "[gpg] SHA256SUMS.asc firmado"
fi

echo "[done] ISO: $ISO_OUT"
ls -lh "$ISO_OUT" "$BUILD_DIR/SHA256SUMS" 2>/dev/null || true
cat "$BUILD_DIR/SHA256SUMS"
