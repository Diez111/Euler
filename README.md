# Euler OS

Debian Testing minimal BTRFS distro — <500MB idle SSD 8GB, nativa 100% Rust (salvo kernel), estilo Mac, base limpia reproducible.

> Base sin entorno gráfico (base limpia Euler, fork anterior /tmp/cosmic-epoch integrado como Euler). Esta capa es la ISO base que validaste: `vm.swappiness=180`, `zram 512M`, `oomd`, `schedulers none/mq-deadline`, `mitigations=auto nosmt preempt=voluntary`.

## Estructura

```
EULER/
├─ crates/euler-core/       lógica particionado GPT LUKS2 BTRFS (tests)
├─ crates/euler-installer/  daemon privilegiado + CLI
├─ config/euler/            package-lists + includes.chroot (grub, systemd, udev)
├─ tools/euler/mkiso.sh     ISO híbrida mmdebstrap + squashfs zstd:19
├─ .github/workflows/euler-iso.yml  CI build-iso + QEMU smoke
└─ Cargo.toml               workspace Euler standalone (no Grafito)
```

## Uso

```bash
sudo ./tools/euler/mkiso.sh --release 2026.09.01
cargo test -p euler-core --lib
cargo run -p euler-installer -- /dev/sda euler euler
```

## Particionado (corregido EFI 512M)

```
GPT
 p1 512M EF00 FAT32 /boot/efi LABEL=EFI
 p2 resto 8309 LUKS2 argon2id -> BTRFS -L EULER --csum xxhash -m dup -d single
     @           -> /        compress=zstd:1,noatime,ssd,discard=async,space_cache=v2,commit=30
     @home       -> /home
     @snapshots  -> /.snapshots
     @var_log    -> /var/log
     @var_cache  -> /var/cache
 tmpfs -> /tmp
```

Ver `crates/euler-core/src/disk.rs:7 EFI_SIZE_MB=512` + `btrfs.rs`.

