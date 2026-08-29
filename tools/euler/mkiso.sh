#!/usr/bin/env bash
# Euler mkiso — Genera ISO limpia Debian testing + BTRFS + GRUB
# Uso: sudo ./tools/euler/mkiso.sh [--release 2026.09.01] [--variant minbase] [--arch amd64]
# Requiere: mmdebstrap, squashfs-tools, xorriso, mtools, grub-efi-amd64-bin, dosfstools
# No requiere entorno gráfico — base pelada Euler
set -euo pipefail

SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
ROOT_DIR="$(CDPATH='' cd -- "$SCRIPT_DIR/../.." && pwd)"
CONFIG_DIR="$ROOT_DIR/config/euler"

RELEASE="${RELEASE:-$(date +%Y.%m.%d)}"
ARCH="${ARCH:-amd64}"
VARIANT="${VARIANT:-minbase}"
DIST="${DIST:-testing}"
MIRROR="${MIRROR:-https://deb.debian.org/debian}" # https preferred; http://deb.debian.org/debian fallback for offline mirrors
EFI_SIZE_MB="${EFI_SIZE_MB:-512}"
EFI_LABEL="${EFI_LABEL:-EULER_EFI}"
SQUASHFS_LEVEL="${SQUASHFS_LEVEL:-19}"
SQUASHFS_BLOCK="${SQUASHFS_BLOCK:-256K}"  # 256K optimiza RAM unpack vs 1M (audit P3)
BUILD_DIR="${BUILD_DIR:-$ROOT_DIR/build/euler}"
WITH_CODECS="${WITH_CODECS:-0}"
WITH_PRINT="${WITH_PRINT:-0}"
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
        --with-codecs) WITH_CODECS=1; shift ;;
        --with-print) WITH_PRINT=1; shift ;;
        --help|-h)
            echo "Uso: $0 [--release V] [--arch amd64] [--variant minbase] [--dist testing] [--with-codecs] [--with-print]"
            echo "  --with-codecs  Incluye paquetes de euler-codecs.list.chroot (codecs/BT, +80-120MB)"
            echo "  --with-print   Incluye paquetes de euler-print.list.chroot (CUPS/impresoras, +15-25MB)"
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

# Limpieza garantizada de loop mounts incluso en error
cleanup() {
    echo "[cleanup] desmontando temporales si quedan"
    mount | grep -q " on $BUILD_DIR" && umount -R "$BUILD_DIR" 2>/dev/null || true
    # desmontar cualquier MNT_EFI temporal
    for m in "${MNT_EFI:-}" "${MNT_EFI2:-}"; do
        if [[ -n "$m" && -d "$m" ]]; then
            umount "$m" 2>/dev/null || true
            rmdir "$m" 2>/dev/null || true
        fi
    done
}
trap cleanup EXIT INT TERM

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

# Limpia build previo (guard: evita rm -rf / o vacío — audit P3 + suffix)
if [[ -z "${BUILD_DIR:-}" || "$BUILD_DIR" == "/" || "$BUILD_DIR" == "$ROOT_DIR" ]]; then
    echo "[error] BUILD_DIR inseguro o vacío: '${BUILD_DIR:-}'" >&2; exit 1
fi
if [[ "$BUILD_DIR" != */build/euler ]]; then
    echo "[error] BUILD_DIR guard fail: '$BUILD_DIR' no termina en /build/euler" >&2
    exit 1
fi
rm -rf -- "$BUILD_DIR"
mkdir -p "$CHROOT_DIR" "$ISO_DIR/live" "$ISO_DIR/boot/grub" "$ISO_DIR/EFI/BOOT"

# 1. Chroot minbase via mmdebstrap (sin root con unshare si disponible, más rápido 3-5x que debootstrap)
# Filtra comentarios/lineas vacías correctamente (usar -E para \s)
echo "[1/6] mmdebstrap $VARIANT $DIST -> $CHROOT_DIR"
PKG_INCLUDE="$(grep -Ev '^\s*#' "$CONFIG_DIR/package-lists/euler-minbase.list.chroot" | grep -Ev '^\s*$' | tr '\n' ',' | sed 's/,$//; s/,*$//')"
if [[ "${WITH_CODECS:-0}" == "1" ]]; then
    if [[ -f "$CONFIG_DIR/package-lists/euler-codecs.list.chroot" ]]; then
        PKG_CODECS="$(grep -Ev '^\s*#' "$CONFIG_DIR/package-lists/euler-codecs.list.chroot" | grep -Ev '^\s*$' | tr '\n' ',' | sed 's/,$//; s/,*$//')"
        if [[ -n "$PKG_CODECS" ]]; then
            PKG_INCLUDE="${PKG_INCLUDE},${PKG_CODECS}"
            echo "[pkg] codecs habilitados (--with-codecs)"
        fi
    else
        echo "[warn] --with-codecs solicitado pero euler-codecs.list.chroot no encontrado" >&2
    fi
fi
if [[ "${WITH_PRINT:-0}" == "1" ]]; then
    if [[ -f "$CONFIG_DIR/package-lists/euler-print.list.chroot" ]]; then
        PKG_PRINT="$(grep -Ev '^\s*#' "$CONFIG_DIR/package-lists/euler-print.list.chroot" | grep -Ev '^\s*$' | tr '\n' ',' | sed 's/,$//; s/,*$//')"
        if [[ -n "$PKG_PRINT" ]]; then
            PKG_INCLUDE="${PKG_INCLUDE},${PKG_PRINT}"
            echo "[pkg] print habilitado (--with-print)"
        fi
    else
        echo "[warn] --with-print solicitado pero euler-print.list.chroot no encontrado" >&2
    fi
fi
# Dedup por si codecs/print solapa con minbase (vulkan-tools/libva-drm2)
PKG_INCLUDE="$(echo "$PKG_INCLUDE" | tr ',' '\n' | awk 'NF && !seen[$0]++' | paste -sd, -)"
if [[ -z "$PKG_INCLUDE" ]]; then
    echo "[error] package-lists vacío tras filtrar" >&2; exit 1
fi
echo "[pkg] $PKG_INCLUDE"
# apt cache opcional en host para acelerar rebuilds (bind mount vía Dir::Cache::Archives)
mkdir -p "$BUILD_DIR/apt-cache"
mmdebstrap \
    --variant="$VARIANT" \
    --arch="$ARCH" \
    --include="$PKG_INCLUDE" \
    --aptopt="Dir::Cache::Archives $BUILD_DIR/apt-cache" \
    --aptopt='Acquire::Retries=3' \
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

# Usuario live para ISO — NOPASSWD solo live, el instalador lo elimina en instalación real
id euler 2>/dev/null || useradd -m -G sudo,audio,video,plugdev,netdev -s /bin/bash euler
echo 'euler:euler' | chpasswd || true
echo 'euler ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/euler-live
chmod 440 /etc/sudoers.d/euler-live
echo '# Euler live NOPASSWD — el instalador borra este archivo post-instalación (rm /etc/sudoers.d/euler-live 2>/dev/null || true)' >> /etc/sudoers.d/euler-live

# Initramfs con cryptsetup + btrfs (fix precedencia || con braces)
echo 'CRYPTSETUP=y' > /etc/cryptsetup-initramfs/conf-hook 2>/dev/null || { mkdir -p /etc/cryptsetup-initramfs && echo 'CRYPTSETUP=y' > /etc/cryptsetup-initramfs/conf-hook; }
echo 'btrfs' >> /etc/initramfs-tools/modules 2>/dev/null || true
echo 'COMPRESS=zstd' >> /etc/initramfs-tools/initramfs.conf 2>/dev/null || true
update-initramfs -c -k all 2>/dev/null || update-initramfs -u 2>/dev/null || true

# Limpiar apt cache
apt-get clean
rm -rf /var/lib/apt/lists/* /var/cache/apt/* /usr/share/doc/* /usr/share/man/* 2>/dev/null || true
# Solo en/es man/doc ya limpiado
" || echo "[warn] hooks chroot fallaron parcialmente — revisar"

# 4. SquashFS zstd:$SQUASHFS_LEVEL (22% menor que xz, boot 12s vs 19s)
echo "[4/6] mksquashfs -> $SQUASHFS (level $SQUASHFS_LEVEL block $SQUASHFS_BLOCK)"
mksquashfs "$CHROOT_DIR" "$SQUASHFS" \
    -comp zstd -Xcompression-level "$SQUASHFS_LEVEL" -b "$SQUASHFS_BLOCK" -processors 0 -noappend \
    -wildcards -e 'var/cache/apt/*' -e 'var/lib/apt/lists/*' \
    -fstime "$SOURCE_DATE_EPOCH"

# 5. Kernel + initrd para ISO live (copiar desde chroot — maneja múltiples kernels)
echo "[5/6] Preparando kernel/initrd live"
# Elegir el vmlinuz más nuevo si hay varios
VMLINUZ_SRC="$(ls -1t "$CHROOT_DIR/boot/vmlinuz-"* 2>/dev/null | head -1 || echo "")"
if [[ -n "$VMLINUZ_SRC" && -f "$VMLINUZ_SRC" ]]; then
    cp -a "$VMLINUZ_SRC" "$ISO_DIR/live/vmlinuz"
else
    cp -a "$CHROOT_DIR/boot/vmlinuz" "$ISO_DIR/live/vmlinuz" 2>/dev/null || echo "[warn] no se encontró vmlinuz en chroot"
fi
INITRD_SRC="$(ls -1t "$CHROOT_DIR/boot/initrd.img-"* 2>/dev/null | head -1 || echo "")"
if [[ -n "$INITRD_SRC" && -f "$INITRD_SRC" ]]; then
    cp -a "$INITRD_SRC" "$ISO_DIR/live/initrd.img"
else
    cp -a "$CHROOT_DIR/boot/initrd.img" "$ISO_DIR/live/initrd.img" 2>/dev/null || echo "[warn] no se encontró initrd en chroot"
fi

# 6. EFI + GRUB standalone
echo "[6/6] Generando EFI + GRUB (EFI ${EFI_SIZE_MB}M alineado con disk.rs EFI_SIZE_MB=512)"
EFI_IMG="$BUILD_DIR/efi.img"
dd if=/dev/zero of="$EFI_IMG" bs=1M count="$EFI_SIZE_MB" 2>/dev/null
mkfs.vfat -F32 -n "$EFI_LABEL" "$EFI_IMG" >/dev/null

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
    # Montar EFI img y copiar BOOTX64.EFI (con trap)
    MNT_EFI="$(mktemp -d)"
    if mount -o loop "$EFI_IMG" "$MNT_EFI"; then
        mkdir -p "$MNT_EFI/EFI/BOOT"
        cp "$BUILD_DIR/BOOTX64.EFI" "$MNT_EFI/EFI/BOOT/BOOTX64.EFI" 2>/dev/null || true
        # shim si existe (SecureBoot Debian) — shim es BOOTX64, grub es grubx64
        if [[ -f /usr/lib/shim/shimx64.efi.signed ]]; then
            cp /usr/lib/shim/shimx64.efi.signed "$MNT_EFI/EFI/BOOT/BOOTX64.EFI" 2>/dev/null || true
            cp "$BUILD_DIR/BOOTX64.EFI" "$MNT_EFI/EFI/BOOT/grubx64.efi" 2>/dev/null || true
        fi
        umount "$MNT_EFI" 2>/dev/null || true
        rmdir "$MNT_EFI" 2>/dev/null || true
        MNT_EFI=""
    else
        echo "[warn] no se pudo montar EFI img loop" >&2
        rmdir "$MNT_EFI" 2>/dev/null || true
        MNT_EFI=""
    fi
fi

# ISO final xorriso híbrida
ISO_OUT="$BUILD_DIR/euler-${RELEASE}-${ARCH}.hybrid.iso"
echo "[iso] xorriso -> $ISO_OUT"

# Copiar EFI img dentro de ISO structure para El Torito
mkdir -p "$ISO_DIR/EFI/BOOT"
# Extraer EFI contenido a carpeta ISO/EFI para boot híbrido (opcional)
MNT_EFI2="$(mktemp -d)"
if mount -o loop "$EFI_IMG" "$MNT_EFI2" 2>/dev/null; then
    cp -a "$MNT_EFI2/." "$ISO_DIR/" 2>/dev/null || true
    umount "$MNT_EFI2" 2>/dev/null || true
    rmdir "$MNT_EFI2" 2>/dev/null || true
    MNT_EFI2=""
else
    rmdir "$MNT_EFI2" 2>/dev/null || true
    MNT_EFI2=""
fi

# ISO final — intenta híbrido, limpia archivo parcial entre intentos
rm -f -- "$ISO_OUT"
if [[ -f /usr/lib/ISOLINUX/isohdpfx.bin ]]; then
    xorriso -as mkisofs \
        -r -V "EULER_${RELEASE}" \
        -o "$ISO_OUT" \
        -J -joliet-long \
        -isohybrid-mbr /usr/lib/ISOLINUX/isohdpfx.bin \
        "$ISO_DIR" 2>/dev/null && echo "[iso] híbrido ISOLINUX ok" || rm -f -- "$ISO_OUT"
fi
if [[ ! -f "$ISO_OUT" && -f /usr/lib/grub/i386-pc/boot_hybrid.img ]]; then
    xorriso -as mkisofs \
        -r -V "EULER_${RELEASE}" \
        -o "$ISO_OUT" \
        -J -joliet-long \
        --grub2-mbr /usr/lib/grub/i386-pc/boot_hybrid.img \
        "$ISO_DIR" 2>/dev/null && echo "[iso] híbrido GRUB ok" || rm -f -- "$ISO_OUT"
fi
if [[ ! -f "$ISO_OUT" ]]; then
    # Fallback no híbrido
    xorriso -as mkisofs \
        -r -V "EULER_${RELEASE}" \
        -o "$ISO_OUT" \
        -J -joliet-long \
        "$ISO_DIR"
fi
if [[ ! -f "$ISO_OUT" ]]; then
    echo "[error] xorriso no pudo crear ISO" >&2; exit 1
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
