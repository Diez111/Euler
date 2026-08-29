# Euler OS — Herramientas ISO

Distro limpia desde `debian-testing-amd64-netinst.iso` base pelada, BTRFS profesional, SSD 8GB, <500MB idle, estilo Mac, todo Rust salvo kernel.

> Entorno gráfico Euler excluido a pedido — se hará como compositor Euler aparte. Esta capa es base reproducible.

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

## Instalador — detección hardware y menú codecs (responsive)

Hardware (`crates/euler-installer/src/gui.rs:90` `render_hardware_card`, `crates/euler-core/src/hw.rs:231` `HwProfile::detect`) y codecs (`gui.rs:118` `render_codec_menu`, `crates/euler-core/src/codecs.rs:35` `CODECS`) comparten layout responsive **360px–1920px**: `terminal_width()` (`gui.rs:73`, respeta `$COLUMNS`, fallback `80` si no tty) → `adaptive_cols(width)` (`gui.rs:79`: `<600→1 col`, `<1024→2`, `≥1024→3`) y **truncate** con `…` (`crates/euler-installer/src/main.rs:196` `truncate_str`: `width.saturating_sub(30).max(10)` para comando y `width/2` para descripción; `gui.rs:128` corta línea a `width` en menú). CLI espejo: `--hw-profile auto|intel|amd|generic|minimal` (`main.rs:96`), `--codecs h264,hevc,webp,heif,av1,vp9,avif,bluetooth,audio-extra|all` (`main.rs:105`), `--detect-hardware` (reporte y exit, `main.rs:93`), `--enable-bluetooth`, `--json`/`--no-encrypt`. Selección valida límite `<500 MiB` (`gui.rs:44` `exceeds_limit`, base 420 MiB + codecs).

## Bootloader

GRUB signed `shim-signed + grub-efi-amd64-signed` (SecureBoot OOTB) + `grub-btrfs` para snapshots Time-Machine. Limine UKI opcional segunda iteración. Base `GRUB_CMDLINE_LINUX_DEFAULT` en `config/euler/includes.chroot/etc/default/grub:6`; per-GPU additions via `HwProfile::kernel_additions()` (`hw.rs:251`) append en instalación (ver `docs/gpu-igpu-optimization.md` §4).

## CI reproducible

`.github/workflows/euler-iso.yml` — `SOURCE_DATE_EPOCH=$(git log -1 --format=%ct)`, `mksquashfs -fstime`, `xorriso -volume_date`, artifact `euler-*.hybrid.iso + SHA256SUMS`.

## Optimización iGPU 100% Rust

Guía completa: [`docs/gpu-igpu-optimization.md`](../../docs/gpu-igpu-optimization.md) — UHD/Xe/Arc + Radeon 780M/Vega, <500 MB idle, <50 MB compositor.

* **Stack:** `wgpu` (safe WebGPU, 5–10% overhead) default + `ash` fallback DMA-BUF + `softbuffer` applets (ver `cosmic-panel#596` WAYLAND_SOCKET caveat). `vulkano` descartado — per-command `HashMap` no escala; `blade` ~0 CPU pero serializa GPU y Zed volvió a `wgpu` 2026. Ver §2 matriz y `smithay#928`/`cosmic-comp#2055`.
* **UMA zero-copy:** `wgpu::Buffer { mapped_at_creation:true }` + `bytemuck::cast_slice` + `RUSTFLAGS=-C target-cpu=x86-64-v3` (ya en `.cargo/config.toml` → AVX2); scanout via `VK_KHR_external_memory_fd` + `VK_EXT_image_drm_format_modifier` + `grafting`/`lamco-wgpu` (`HostWgpuContext` shared `ash::Device` con Smithay, sin duplicar instance).
* **Kernel:** añadir a `GRUB_CMDLINE_LINUX_DEFAULT` → `i915.enable_guc=3 i915.enable_fbc=1 i915.enable_psr=2 amdgpu.ppfeaturemask=0xffffffff amd_pstate=active pcie_aspm=force mem_sleep_default=deep ahci.mobile_lpm_policy=3` (THP `madvise` ya). Ver §4 tabla.
* **Paquetes:** `mesa-vulkan-drivers` (ANV+RADV) + `mesa-va-drivers` + **`vulkan-tools` + `libva-drm2`** (nuevos en `euler-minbase.list.chroot`, +1.2 MB) + `libgl1-mesa-dri`/`libglx-mesa0`; `firmware-*`+`intel-microcode`/`amd64-microcode` ya cubre GuC/HuC. Ver §5.
* **Targets:** `free <500000` KB, `euler-comp RSS <50 MB`, `memfd <50` steady (`validate-iso.sh` cosmic-comp→euler-comp, §6 soak `vkcube --c 1000`).
* **Roadmap `euler-comp`:** `smithay 0.5` DRM/GBM + `lamco-wgpu` bridge para DMA-BUF import → `naga` WGSL blur/shadow → panel `softbuffer` → fork `euler-session`. Ver §7 fases y `Cargo.toml` esqueleto `wgpu 29`/`grafting 0.5`/`softbuffer 0.4`/`ash 0.38`.

Validar:

```bash
grep -E "mesa-vulkan|vulkan-tools|libva" config/euler/package-lists/euler-minbase.list.chroot
grep CMDLINE config/euler/includes.chroot/etc/default/grub
cat docs/gpu-igpu-optimization.md | head -80
vulkaninfo --summary | grep -E "Heap|GPU" && vainfo | head
```

## Menús hardware y codecs — responsive + CLI

Interfaces 100% Rust sin Slint nativo (placeholder responsive, ver `crates/euler-installer/src/gui.rs:1`).

* **Detección hardware** — `crates/euler-core/src/hw.rs:231` `HwProfile::detect()` (lspci/lsusb + `/sys/class/drm`, `/proc/cpuinfo`, `/proc/meminfo`; `vendor_to_gpu` 0x8086 Intel / 0x1002 AMD / 0x10de Nvidia). `crates/euler-installer/src/gui.rs:90` `render_hardware_card(hw, width)` muestra `CPU / RAM / GPU / WIFI / BT / NVMe` (6 cards) con `HwProfile::kernel_additions()` hint. CLI: `euler-installer /dev/sda euler euler --hw-profile auto|intel|amd|generic|minimal --detect-hardware` (`crates/euler-installer/src/main.rs:96` `parse_flags` + `validate_hw_profile`) y reporte `print_hardware_report_and_exit`. En ISO build, `mkiso.sh --with-codecs` incluye `euler-codecs.list.chroot` (ver `tools/euler/mkiso.sh:34`).

* **Codecs / Bluetooth** — `crates/euler-core/src/codecs.rs:35` tabla const `CODECS` (9 entradas `&'static [CodecOption]`, `CODECS[i].size_mb/packages`, zero-copy) y `crates/euler-installer/src/gui.rs:20` `CodecSelection::new()` (12 opciones `gstreamer1.0-libav 3M`, `bluez 2M`, `libavcodec-extra 15M`...). `render_codec_menu(codecs, width)` (`gui.rs:118`) lista `[x]/[ ] name size badge + desc`, `total_size_mb()` / `estimated_iso_mb() = 420 + total` / `exceeds_limit() >500` con warning `⚠️ ISO >500 MiB`. CLI: `--codecs h264,hevc,webp,heif|all` (`main.rs:105` `parse_codecs_value` valida contra `validate_codec_id`, dedup) + `--enable-bluetooth` (`hw.extra_packages()` → `bluez/bluez-firmware`). En `crates/euler-core/src/install.rs:347` `push_hw_packages(hw, codecs, bluetooth)` inyecta `InstallStepKind::HwPackages` (y `push_fstab_entries` en `install.rs:414` usa `crates/euler-core/src/btrfs.rs:134` `fstab_entries_for_hw` para toglear `ssd`/`discard` si `has_nvme=false`).

* **Responsive 360px–1920px** — `gui.rs:73` `terminal_width()` lee `COLUMNS` env o `is_terminal()` fallback `80` (non-tty → 80), `gui.rs:79` `adaptive_cols(width)` → `1` si `width<600`, `2` si `width<1024`, `3` si `>=1024` (cobertura probada 360–1920 en `gui.rs:152` tests `adaptive_cols_breakpoints` 400/599/600/1023/1024/2000). `render_hardware_card` y `render_codec_menu` usan `cols` para `cards.chunks(cols)` (`[  CARD  ]` grid 1/2/3) y `format!("Hardware ({} cols — {}px)", cols, width)`. `main.rs:196` `truncate_str(s, max)` + `gui.rs:128` `if line.len() > width && width>20 { &line[..width] }` y `main.rs:238` `truncate_str(&cmd_str, width.saturating_sub(30).max(10))` / `width/2` para descripción, garantizan no overflow en 360px (ver `render_does_not_panic` con 80 cols).

Validar:

```bash
cargo test -p euler-installer -- gui     # adaptive_cols, codec_total, render, terminal_width
COLUMNS=360 cargo run -p euler-installer -- /dev/sda euler euler --detect-hardware
COLUMNS=1920 cargo run -p euler-installer -- /dev/sda euler euler --hw-profile intel --codecs h264,hevc --enable-bluetooth
./tools/euler/mkiso.sh --with-codecs --release 2026.09.01  # +1.2M vulkan-tools/libva-drm2 + codecs solap
```

## Scandinavian Design System

Sistema light minimal 100% tokens en Rust — dark `#0a0a0f`/`#E8E8E8`/`#a3a3a3` neutralizado (0 hits en `config/` — ver `theme.rs:1`, `theme.txt:3`).

* **Canvas & ink:** `canvas #FFFFFF` (`theme.rs:7` `CANVAS`/`SURFACE`), `ink #000000` con alphas `87%` `rgba(0,0,0,0.87)` / `60%` / `38%` / `12%` / `06%` / `04%` (`theme.rs:14` `INK_87`…`INK_04`) — texto `INK_87`, secundario `INK_60`, muted/caption `INK_38`, bordes `INK_12`, fills `INK_06`/`INK_04`. GRUB espejo `theme.txt:3` `desktop-color #FFFFFF`, `title #000000`, `item #00000099` (60%), `selected #000000DE` (87%) sobre `fill #0000000F` (6%).
* **Ritmo 8px:** `SPACING=8` (`theme.rs:48`), `RADIUS=12px` (`theme.rs:41`), `SHADOW 0 8px 32px rgba(0,0,0,0.08)` (`theme.rs:45`), `item_height 40` + `item_spacing 8` + `padding 12` en `theme.txt:12`.
* **Márgenes & gaps:** `MARGIN 24/40/64 px` mobile/tablet/desktop (`theme.rs:51` `MARGIN_*_PX`) → mapeo TUI `margin(width)` `3/4/8` chars (`theme.rs:69` `<600→3`, `<1024→4`, `≥1024→8`); `SECTION_GAP 96/144 px` (`theme.rs:56`) → `section_gap(width)` `3/5` líneas (`theme.rs:82` `<600→3` else `5`). Usados en `gui.rs:111` `rule_len = (width - margin*2).clamp(8,48)` y `gui.rs:115,150` `"\n".repeat(section_gap)`.
* **Tipografía:** `Inter Variable` + `Noto Sans` fallback (`theme.rs:64` `FONT_SANS`), jerarquía `h1 32/40 500`, `h2 20/28 500`, `caption 12 muted INK_38`, `body 14/20` — left-aligned, tabular (`gui.rs:4` comentario, `gui.rs:110` header).
* **Focus & interacción:** `focus-ring` doble `2px #FFFFFF` + `4px INK_87` (`theme.rs:36` `FOCUS_RING: 0 0 0 2px #FFFFFF, 0 0 0 6px rgba(0,0,0,0.87)` / `INNER`/`OUTER`), `hover 5%` `rgba(0,0,0,0.05)` y `pressed 9%` `rgba(0,0,0,0.09)` (`theme.rs:30` `HOVER_ALPHA`/`PRESSED_ALPHA`). `prefers_reduced_motion()` respeta `EULER_REDUCED_MOTION`/`NO_ANIM` (`theme.rs:92`).
* **Neutralización dark:** antiguo `bg #0a0a0f`, `label #E8E8E8`, `idle #a3a3a3` reemplazado; `grep -r "#0a0a0f\|#E8E8E8\|#a3a3a3" config/` → `0` (solo docs históricos). `background.png` neutralizado a sólido blanco + grid 4% (`theme.txt:25`).
* **Responsive 360–1920 intacto:** `terminal_width()` (`gui.rs:80` respeta `$COLUMNS`, fallback `80` si no tty) → `adaptive_cols(width)` (`gui.rs:87`: `<600→1`, `<1024→2`, `≥1024→3` — con guardas `<360`/`<768`/`<1280` equivalentes, probado `0→2000` en `gui.rs:188` tests) y truncate Unicode `…` (`theme.rs:101` `truncate_str` con `max≤1` sin ellipsis, `gui.rs:157` `width>20` guard). Hardware card `chunks(cols)` + codec menú `truncate_str(&line,width)` garantizan no overflow en 360px. Ver `docs/gpu-igpu-optimization.md` Apéndices A/B y § Instalador.

Archivos fuente: `crates/euler-installer/src/theme.rs` (tokens), `crates/euler-installer/src/gui.rs` (`adaptive_cols`, `margin`, `section_gap`, `render_*`, `truncate_str`), `crates/euler-installer/src/main.rs:196` `truncate_str` CLI espejo (`width.saturating_sub(30).max(10)` / `width/2`), `config/euler/grub/themes/euler/theme.txt` (GRUB palette).

## Post-instalación — euler-config

`tools/euler/euler-config` — toggles post-instalación sin reinstalar, Scandinavian minimal (help left-aligned, comentarios cortos).

* **Printer (CUPS):** `cmd_printer_enable()` → `systemctl unmask` + `systemctl enable --now cups.service cups-browsed.service` + `modprobe usblp 2>/dev/null || true`; `cmd_printer_disable()` → `systemctl disable --now cups.service cups-browsed.service`; `cmd_printer_status()` → `is-enabled`/`is-active` para `cups`/`cups-browsed` + `lsmod | grep ^usblp` (ver `tools/euler/euler-config:34`).
* **Bluetooth (BlueZ):** espejo consistente `cmd_bluetooth_enable()` → `systemctl unmask` + `rfkill unblock bluetooth` + `systemctl enable --now bluetooth.service` + `modprobe btusb`; `disable` → `disable --now`; `status` → `is-enabled`/`is-active` + `rfkill` + `btusb` (`euler-config:66`).
* **CLI:** `sudo euler-config printer enable|disable|status` y `sudo euler-config bluetooth enable|disable|status` (`euler-config:90` `main` dispatch `printer`/`bluetooth` + `help|--help|-h`). Requiere root para `enable`/`disable`, `status` sin root.

Validar:

```bash
./tools/euler/euler-config help
./tools/euler/euler-config printer status
sudo ./tools/euler/euler-config printer enable
sudo ./tools/euler/euler-config printer disable
sudo ./tools/euler/euler-config bluetooth status
grep -n "cmd_printer_enable\|systemctl enable --now cups" tools/euler/euler-config
```

## Próximos pasos (compositor Euler)

* Parchear `cosmic-comp#2073 memfd leak` (antes cosmic-comp, ahora euler-comp) y `#2265` antes de medir <500MB
* `euler-panel` (antes cosmic-panel) + 3 applets Slint softbuffer, `swaybg 3MB`, `greetd 5MB`
