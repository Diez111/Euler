# Euler OS — Herramientas ISO

Distro limpia desde `debian-testing-amd64-netinst.iso` base pelada, BTRFS profesional, SSD 8GB, <500MB idle, estilo Mac, todo Rust salvo kernel.

> COSMIC está excluido a pedido — se hará fork aparte. Esta capa es base reproducible.

## Estructura

```
tools/euler/
  mkiso.sh          # Genera ISO híbrida mmdebstrap + squashfs zstd:19 + xorriso
  validate-iso.sh   # Valida post-boot <500MB, BTRFS, zram, oomd, schedulers
  repo/setup-aptly.sh  # Repo APT aptly + GPG euler@euler.bo
config/euler/
  package-lists/euler-minbase.list.chroot  # Paquetes minbase exactos
  includes.chroot/etc/
    sysctl.d/99-euler.conf         vm.swappiness=180 etc
    systemd/{zram-generator.conf,journald.conf,coredump.conf,system.conf.d/10-euler.conf,oomd.conf.d/,system-preset/70-euler.preset}
    udev/rules.d/60-ioschedulers.rules  nvme none, sda mq-deadline
    default/grub                  mitigations=auto,nosmt preempt=voluntary
    fstab, crypttab, os-release, hosts, plymouth
  grub/themes/euler/theme.txt     Tema GRUB Mac blur
crates/euler-core/                Lógica particionado GPT + LUKS2 + BTRFS (tests)
crates/euler-installer/           Daemon privilegiado + CLI plan (tests)
```

## Uso rápido

```bash
# 1. Build ISO (requiere root, mmdebstrap, squashfs-tools, xorriso, mtools, grub-efi)
sudo ./tools/euler/mkiso.sh --release 2026.09.01

# 2. Validar ISO booteada
sudo ./tools/euler/validate-iso.sh

# 3. Probar instalador (dry-run, no destruye)
cargo run -p euler-installer -- /dev/sda euler euler | head -40
echo '{"device":"/dev/sda","hostname":"euler","username":"euler","password":"euler","encrypt":true}' | cargo run -p euler-installer-daemon --

# 4. Repo APT
./tools/euler/repo/setup-aptly.sh
aptly repo add euler build/*.deb
aptly publish repo -gpg-key="euler@euler.bo" -distribution=testing euler
```

## Particionado profesional

```
GPT
 p1 1024M EF00 FAT32 /boot/efi LABEL=EFI
 p2 resto 8309 LUKS2 argon2id -> BTRFS
     @              -> /           compress=zstd:1,noatime,ssd,discard=async,space_cache=v2,commit=30
     @home         -> /home
     @snapshots    -> /.snapshots
     @var_log      -> /var/log
     @var_cache    -> /var/cache
 tmpfs -> /tmp (zram 512M)
```

EFI 1G deja 450M libres para UKIs; `compress=zstd:1` 2.5x ratio <3% CPU; `discard=async` 0 stall.

## Tuning <500MB

* `zram 4G zstd prio 100` + `swappiness 180` (sin 180 zram inútil)
* `transparent_hugepage=madvise` ahorra 40-120MB vs always
* `journald volatile 32M`, `coredump none`, mask tracker/cups/avahi/ModemManager/colord = -52MB
* `scheduler none` NVMe, `mq-deadline` SATA, `noatime`
* `systemd-oomd` 2MB (no nohang 15MB)

Validar con:
```bash
free | awk '/Mem:/ {print $3}' # <500000
smem -t -k | tail
zramctl && cat /proc/swaps
compsize / && btrfs filesystem usage /
```

## Bootloader

GRUB signed `shim-signed + grub-efi-amd64-signed` (SecureBoot OOTB) + `grub-btrfs` para snapshots Time-Machine. Limine UKI opcional segunda iteración.

## CI reproducible

`.github/workflows/euler-iso.yml` — `SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)`, `mksquashfs -fstime`, `xorriso -volume_date`, artifact `euler-*.hybrid.iso + SHA256SUMS`.

## Próximos pasos (cuando fork COSMIC)

* Parchear `cosmic-comp#2073 memfd leak` y `#2265` antes de medir <500MB
* `cosmic-panel` + 3 applets Slint softbuffer, `swaybg 3MB`, `greetd 5MB`
