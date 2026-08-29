# Optimización iGPU 100% Rust — Euler OS

> **Foco:** Intel UHD/Xe/Arc iGPU y AMD Radeon 780M/Vega primero, dedicadas después.
> **Objetivo ISO:** `<500 MB idle` · **Compositor:** `<50 MB RSS` · **FD leak:** `<50 memfd` · **Stack:** 100% Rust, sin C++.
> **Validado contra:** `config/euler/package-lists/euler-minbase.list.chroot`, `config/euler/includes.chroot/etc/default/grub`, `config/euler/includes.chroot/etc/sysctl.d/99-euler.conf`, `Cargo.toml` workspace (`rust-version=1.88`, `RUSTFLAGS=-C target-cpu=x86-64-v3` en `.cargo/config.toml`).

---

## 1. Integrada vs dedicada — arquitectura unified memory

### 1.1 Física del hardware

|  | iGPU (UHD 620-770, Xe, Arc iGPU; Vega 8/11, 780M RDNA3) | dGPU (Arc A/NVIDIA/AMD RX) |
|---|---|---|
| **Memoria** | **Unified / UMA**: roba del DDR4/DDR5 del host. No VRAM separada. BIOS reserva 128 MB–2 GB (stolen), resto dinámico vía GTT. `lspci -v` → `Memory at ... prefetchable`. | VRAM dedicada GDDR6 + BAR ReBAR. Copia PCIe obligatoria host↔device. |
| **Ancho de banda** | Comparte bus RAM: DDR5-4800 ≈ 38 GB/s por canal (≈76 GB/s dual) — compite con CPU. Latencia baja, coherencia LLC. | GDDR6 16 Gbps × 128-bit = 256 GB/s exclusivo, + PCIe 4.0 x8 16 GB/s para uploads. |
| **Coherencia** | `CLFLUSH` no necesario en Intel LLC coherente (hsw+), AMD APU `amdgpu` con `GTT` coherente. `VK_MEMORY_PROPERTY_HOST_COHERENT_BIT` casi gratis. | Requiere `vkFlushMappedMemoryRanges` si non-coherent. |
| **Pag Tabla** | GTT / GGTT mapea páginas host directo. Sin bounce buffer si driver usa `TTM` + `shmem`. | VRAM = local, GTT = visible, SHM = host copy. |
| **Power** | 5–28 W cTDP compartido CPU+iGPU. Freq escalado `intel_pstate` / `amd_pstate` conjunto. | 75–450 W separado, ASPM + `pcie_aspm=off` nuance. |

### 1.2 Implicancia para Euler `<500 MB`

- iGPU **no añade copia VRAM** si el compositor usa buffers scanout correctos (DMA-BUF). Cada frame 1920×1080×4 = **7.9 MB**. Doble buffer + cursor = ~17 MB. En dGPU ese mismo frame existiría 2× (host staging + VRAM). En iGPU es **1× si zero-copy**.
- Presupuesto: 500 MB idle − 17 MB scanout − 40 MB compositor = **443 MB para resto del sistema** (systemd, pipewire, NM). Compositor `<50 MB` es viable solo si no duplica buffers y no hace compositing en CPU (softbuffer full-screen).
- `zram 4G zstd` ya en `99-euler.conf: vm.swappiness=180` es correcto: iGPU bajo presión RAM comprime antes que swappear a NVMe, evita stalls en `GTT` faults.

### 1.3 Verificación host

```bash
# Intel
lspci -nn | grep -i vga
dmesg | grep -i i915 | head -20
cat /sys/kernel/debug/dri/0/i915_gem_objects 2>/dev/null | head

# AMD
dmesg | grep -i amdgpu | head -20
cat /sys/kernel/debug/dri/0/amdgpu_gem_info 2>/dev/null | head
glxinfo -B | grep -E "renderer|VRAM|UMA"
vulkaninfo --summary | grep -A2 "Heap\|Type"
cat /proc/meminfo | grep -E "MemTotal|Shmem|DirectMap"
```

---

## 2. Patrones Rust 100% — `wgpu` vs `ash` vs `vulkano` vs `softbuffer`

Investigación 2026 (websearch + fuentes `gfx-rs/wgpu`, `ash-rs/ash`, `vulkano-rs/vulkano`, `rust-windowing/softbuffer`, `Smithay`, `sparkles-docs/pages.dev/research/vulkan/comparison`).

### 2.1 Matriz comparativa (100% Rust, sin C++)

| Crate | Tipo | Safety | Backends | Overhead CPU vs raw `ash/hal` | Estado 2026 | Recomendado Euler |
|---|---|---|---|---|---|---|
| **`wgpu` 29.x** | Safe WebGPU wrapper sobre `wgpu-hal`→`ash` | Total: lifetimes + `Send+Sync` en `Arc`, validación host always-on, `naga` valida WGSL → SPIR-V/MSL/HLSL, 1.5 ms/shader. | Vulkan, D3D12, Metal, GL, Web | **5–10 % típico, ~2× worst** (`wgpu#2080`, `wgpu#5525` lock contention). `v0.19 arcanization` Arc ganó +45% en Bevy parallel encoding. | Muy activo (Firefox, Bevy, Servo-wgpu-interop). `gfx-hal` retirado 2021, ahora `wgpu-hal` directo sobre `ash`. | **SÍ — default** para compositor y apps 3D. Portátil, mejor docs, menos riesgo de mantenimiento. |
| **`ash` 0.38** | Thin unsafe FFI generado desde `vk.xml` | Ninguna: *everything is unsafe*, `repr(transparent)` newtypes, `Extends/pNext` lifetimes. | Solo Vulkan | **Zero-cost**: `#[inline]` sobre tabla fn ptr cacheada. Coste = validation layers (~10-30% si activas). | De-facto standard pero **mantenimiento frágil**: generador 3k líneas spaghetti, pocos mantenedores, releases esporádicos tras 2024, sin Vulkan 1.4 hasta Q2 2026. | **SÍ — fallback** para hot path y DMA-BUF (`VK_KHR_external_memory_fd`, `VK_EXT_image_drm_format_modifier`) que `wgpu` no expone estable. |
| **`vulkano` 0.35+`** | Safety-first wrapper (auto-sync) | Auto-sync per-resource `HashMap<Arc,RangeMap>` + host validation always-on. v0.35 migra a `vulkano-taskgraph` (declared DAG) porque hash/range por comando no escalaba. | Solo Vulkan | Runtime tracking per command, similar a `wgpu` (~5-10%). Taskgraph compila DAG una vez, no por comando. Banner: *EXPERIMENTAL, no validation*. | Activo pero gen2 → taskgraph rompe API. `erupt` alternativa muerta 2022. | **NO para Euler 1.0** — overhead similar + menor ecosistema + API inestable. Evaluar v0.36+ si taskgraph madura. |
| **`softbuffer` 0.4+** | CPU raster → `present()` | Safe, sin GPU | Wayland Tier1, X11 Tier1, DRM/KMS Tier3 | **0 GPU**: `memcpy` a SHM. 1080p blit ≈ 3-5 ms/frame en x86-64-v3. Sin `libvulkan`. | Usado por `libcosmic` applets por defecto (fallback). `wgpu` feature opcional en `libcosmic`. | **SÍ — applets COSMIC-like** (<1 MB RSS c/u). Ver §2.3. |
| `blade` | Zero-tracking (`GENERAL` layout) | Ninguna, 1 global barrier entre passes | Vulkan | ~0 CPU pero serializa GPU. `bunnymark` 18-23K draws vs `wgpu-hal` 60K. Autor de `wgpu` (kvark) lo abandonó: Zed volvió a `wgpu` 2026 por driver freezes. | Nicho, sin tracker. | No — lección: *no reinventar compositor sin barriers*. |

**Conclusión Euler:** `wgpu` + `ash` fallback + `softbuffer` para applets. Un solo `wgpu-hal` subyacente evita duplicar `ash` vs `vk-sys`.

### 2.2 Por qué `wgpu` como default

- **Ownership Rust = seguridad GPU:** `wgpu::Buffer` lifetime codifica host vs device, imposible `use-after-free` en compile time, `Send+Sync` seguro cross-thread (Smithay render thread).
- **Portabilidad:** mismo `wgsl` corre en Vulkan/dGPU y en iGPU sin `VK_KHR_portability`. Para ISO única, probado en Intel ANV + RADV sin bifurcar.
- **Naga:** valida shaders por tipo/mem safety/UB antes del driver — menos `GPU hang` que `ash` raw (crítico en <500 MB sin coredump: `systemd/coredump.conf Storage=none` ya).
- **Escape hatch `hal`:** `device.as_hal::<api::Vulkan>()` expone `ash::Device` + `create_texture_from_hal` para DMA-BUF cuando `wgpu` high-level no alcanza (ver §3.3). Ruta usada por `grafting` y `lamco-wgpu` (wgpu como guest de Smithay).

### 2.3 `softbuffer` para applets COSMIC-like

`cosmic-panel#596` resume el trade-off real medido 2026:

- Applets COSMIC oficiales usan `softbuffer` por defecto (`libcosmic` sin feature `wgpu`). `WAYLAND_SOCKET` inyectado rompe `wgpu`/`Vulkan` surface (`ERROR_SURFACE_LOST_KHR`, RADV rechaza adapter) mientras `softbuffer` sigue funcionando (pero sin `DRI`, solo `SHM`).
- Perf: lista scrolleable con `softbuffer` ~1 fps/scroll update vs 60 fps con `wgpu` (RADV reportado). Para applets simples (reloj, tray) `softbuffer` 0.3-0.8 MB RSS es ideal; para animaciones/blur/imagenes, `wgpu` gana 10×.
- **Regla Euler:** panel + applets ≤1 MB c/u → `softbuffer`. App/overlay/compositor principal → `wgpu`. No mezclar `WAYLAND_SOCKET` + `wgpu` sin documentar `X_PRIVILEGED_WAYLAND_SOCKET` / `X-HostWaylandDisplay`.

**Cargo feature sugerido** (`crates/euler-comp/Cargo.toml` futuro):

```toml
[features]
default = ["wgpu"]
wgpu = ["dep:wgpu", "dep:naga"]
softbuffer-applets = ["dep:softbuffer", "dep:winit"]
ash-fallback = ["dep:ash", "dep:ash-window"]
```

### 2.4 Anti-patrones a evitar

- `vulkano` per-command `HashMap` sin `taskgraph`: no escala en compositor 60 fps.
- `blade` global barrier: oculta UB in-pass, serializa—revierte ganancias UMA.
- `wgpu` + `ash` mix sin reset de tracking: *partial opt-out invalida tracking* (sparkles survey). Si usas `hal` escape, aísla el `Device` (`HostWgpuContext`) o invalida `wgpu` state.

---

## 3. Buffer handling — zero-copy en iGPU

### 3.1 `wgpu::Buffer` — el contrato UMA

`wgpu` abstrae `host-visible` vs `device-local` pero en iGPU ambas son RAM:

```
CPU view:  get_mapped_range() -> &mut [u8]   (MAP_WRITE|MAP_READ)
GPU view:  queue.submit() -> use via bind group (STORAGE/VERTEX/UNIFORM)
Transición: mapped ↔ unmapped, nunca ambos. En UMA no hay DMA copy, solo barrier + TLB flush.
```

Patrones:

```rust
// 1) mappedAtCreation — inicialización una vez, zero staging si UMA (WGPU 29)
use wgpu::{BufferUsages, BufferDescriptor};
use bytemuck::{Pod, Zeroable};

#[repr(C)] #[derive(Copy, Clone, Pod, Zeroable)]
struct Vertex { pos: [f32; 3], col: [f32; 3] }

let buf = device.create_buffer(&BufferDescriptor {
    label: Some("verts"),
    size: (verts.len() * std::mem::size_of::<Vertex>()) as u64,
    usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
    mapped_at_creation: true,
});
buf.slice(..).get_mapped_range_mut().copy_from_slice(bytemuck::cast_slice(&verts));
buf.unmap(); // ahora GPU-owned. En iGPU no copia, solo cambia ownership + cache flush

// 2) queue.write_buffer — staging gestionado por wgpu (usa bounce si dGPU, alias si iGPU)
queue.write_buffer(&buf, 0, bytemuck::cast_slice(&verts));

// 3) Lectura async (staging MAP_READ)
let staging = device.create_buffer(&BufferDescriptor {
    label: Some("readback"),
    size: buf.size(),
    usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
    mapped_at_creation: false,
});
let mut enc = device.create_command_encoder(&Default::default());
enc.copy_buffer_to_buffer(&buf, 0, &staging, 0, buf.size());
queue.submit([enc.finish()]);
let (tx, rx) = futures_channel::oneshot::channel();
staging.slice(..).map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
device.poll(wgpu::Maintain::Wait);
rx.await.unwrap().unwrap();
let data: &[u8] = &staging.slice(..).get_mapped_range();
// En iGPU 780M el copy_buffer_to_buffer puede ser no-op si driver elide copy a MAP_READ heap.
```

**Regla:** en compositor evita `MAP_READ` por frame. Usa `COPY_DST|VERTEX|INDEX` dedicados por frame, double-buffered.

### 3.2 `bytemuck` — cast zero-copy

`bytemuck::cast_slice` es `transmute` seguro solo si `Pod + Zeroable` (repr C, sin padding no iniciado). Euler ya fija `RUSTFLAGS=-C target-cpu=x86-64-v3` en `.cargo/config.toml` → `AVX2 + FMA + BMI2` disponibles para `bytemuck`/`naga`/`wgpu` SIMD sin runtime dispatch. `clip` y `mesa` no necesitan bindings C++.

```toml
# Cargo.toml workspace ya: edition 2021, rust-version 1.88
[dependencies]
bytemuck = { version = "1.19", features = ["derive"] }
wgpu = "29"
```

Pitfall: `#[repr(C)]` obligatorio; `#[repr(Rust)]` + `Pod` es UB. Usa `cargo clippy -- -D clippy::transmute_undefined_repr`.

### 3.3 Zero-copy real — DMA-BUF + `VK_KHR_external_memory`

Para compositor Wayland (Smithay) el frame del cliente es un `dma_buf_fd` (DRM). Importar sin copiar:

```
Cliente (Firefox/MPV) —render→ GBM BO → dma_buf_fd ──SCM_RIGHTS──► Compositor (Smithay/wgpu)
                                                              │
                              VK_EXT_image_drm_format_modifier + VK_KHR_external_memory_fd
                              vkCreateImage(tiling=DRM_FORMAT_MODIFIER_EXT, handleType=DMA_BUF_EXT)
                              vkAllocateMemory + ImportMemoryFdInfoKHR(fd) + vkBindImageMemory
                              create_texture_from_hal::<Vulkan>(raw Image)
                                                              ↓
                                                      wgpu::Texture (RESOURCE)
                                                      sample en shader → scanout
```

Código referencia (extracto `grafting 0.5` / `wgpu-native-texture-interop`):

```rust
// Requiere wgpu Device con VK_EXT_image_drm_format_modifier habilitada.
// wgpu default no la habilita — usar create_dmabuf_host_context:
let host = grafting::create_dmabuf_host_context(&adapter, &wgpu::DeviceDescriptor::default())?;
// host.device es wgpu::Device + queue con extension extra

// Import fd recibido por wayland-drm / linux-dmabuf-v1
let tex: wgpu::Texture = grafting::import_vulkan_external_image(
    &VulkanExternalImage { dmabuf_fd, size, format: wgpu::TextureFormat::Bgra8UnormSrgb,
                           drm_modifier, dmabuf_stride, dmabuf_offset },
    &host,
)?;
```

Crates listos 2026:
- `grafting` 0.5 (`wgpu-29` feature) — import DMABUF→Vulkan→wgpu en Linux.
- `lamco-wgpu` 2026-02 — `wgpu` como guest de Smithay (shared Vulkan instance, sin duplicar `ash`).
- `wgpu-dma-buf` (bits-craft) — EGL path legado `eglExportDMABUFImageMESA` si `WGPU_BACKEND=gl`.

**Smithay ya usa `ash` para GBM allocator** (`smithay#928` / `cosmic-comp#2055`): el plan upstream es `ash` directo para renderer Vulkan, no `wgpu` completo. Euler puede seguir `lamco-wgpu` bridge para reúsar `wgpu` shaders.

### 3.4 `target-cpu=x86-64-v3` y mesa

`.cargo/config.toml` ya:

```toml
[build]
rustflags = ["-C", "target-cpu=x86-64-v3"]
incremental = false
```

Por qué `v3` (Haswell+ 2013): AVX2+FMA+BMI1/2+LZCNT+MOVBE. Para Euler minbase (testing, 2026) descarta <2% del parque (pre-2013) pero gana 15-30% en `blit` de `softbuffer`, `zstd:1` del BTRFS, y `naga` SPIR-V→MSL transpila. `mesa` ya compila con `x86-64-v2` en Debian, no conflict.

`mesa` deps relevantes: `libgl1-mesa-dri` (DRI loader), `libglx-mesa0` (GLX), `mesa-vulkan-drivers` (RADV+ANV+lavapipe), `mesa-va-drivers` (VA-API para 780M/UHD decode). En iGPU `LIBVA_DRIVER_NAME=radeonsi`/`iHD` auto.

---

## 4. Kernel params para iGPU

### 4.1 Estado actual Euler (`includes.chroot/etc/default/grub:6`)

```
quiet splash mitigations=auto,nosmt preempt=voluntary
transparent_hugepage=madvise init_on_alloc=0 init_on_free=0
zswap.enabled=0 nowatchdog nmi_watchdog=0
nvme_core.default_ps_max_latency_us=0
```

`transparent_hugepage=madvise` **ya** ahorra 40-120 MB vs `always` (evita THP 2 MB por `mmap` anónimo de buffers GTT). `zswap.enabled=0` correcto con `zram` (no doble compresión).

### 4.2 Recomendados específicos iGPU (añadir a esa línea)

```
i915.enable_guc=3  i915.enable_fbc=1  i915.enable_psr=2
amdgpu.ppfeaturemask=0xffffffff  amdgpu.abmlevel=0
amd_pstate=active  amd_pstate.shared_mem=1
mem_sleep_default=deep  pcie_aspm=force  ahci.mobile_lpm_policy=3
```

| Param | HW | Detalle | Ganancia / Riesgo |
|---|---|---|---|
| `i915.enable_guc=3` | Intel Gen12+ (ADL, RPL, MTL, LNL) | Bit1=GuC load, Bit2=HuC auth+submission. Offloada scheduling a GuC firmware + HuC para HEVC. Desde kernel 6.8 default es `3` en MTL+ pero `0` en TGL/RPL-S; forzar `3` unifica. Requiere `firmware-misc-nonfree` (ya) con `intel-ucode` (ya). | +5-10% FPS, menos `ksoftirqd` jitter. Si firmware ausente, dmesg `GuC fw load failed` → fallback a `0` safe. |
| `i915.enable_fbc=1` | Intel | Framebuffer compression (lossless). Ahorra BW DRAM ~30% en idle compositor. | −0.5 W idle. Raro artifact en panel MIPI. |
| `i915.enable_psr=2` (`PSR2`) | Intel eDP | Panel Self Refresh selective update (solo dirty regions). | −1-2 W en laptop idle. Test `intel_reg read` PSR. |
| `amdgpu.ppfeaturemask=0xffffffff` | AMD | Unlocka todas `PP_FEATURE_MASK` (overclock, fan, deep sleep). Default estable es recortado; mask completa permite `power_dpm_force_performance_level`. | Permite `echo manual > power_dpm_force_performance_level` para tuning. Sin userpace, no efecto negativo. |
| `amd_pstate=active` | AMD Zen3+ (780M) | CPPC active mode. Reemplaza `acpi-cpufreq`. Colabora con `amdgpu` para `shared_mem` freq. | +15% perf/W. Requiere kernel ≥6.5, ya en testing 6.11+. |
| `amdgpu.abmlevel=0` | AMD laptop | Desactiva Adaptive Backlight Management si causa flicker con `wgpu` HDR. `0`=off, `1-4`=niveles. | Opcional; default `0` recomendado para compositor. |
| `pcie_aspm=force` | Ambos | Fuerza L1 ASPM en dGPU/iGPU gen. | −0.8 W idle PCIe. Validar con `powertop` no clock gating broken. |

**Línea final sugerida** (`/etc/default/grub`):

```ini
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash mitigations=auto,nosmt preempt=voluntary transparent_hugepage=madvise init_on_alloc=0 init_on_free=0 zswap.enabled=0 nowatchdog nmi_watchdog=0 nvme_core.default_ps_max_latency_us=0 i915.enable_guc=3 i915.enable_fbc=1 i915.enable_psr=2 amdgpu.ppfeaturemask=0xffffffff amd_pstate=active pcie_aspm=force mem_sleep_default=deep ahci.mobile_lpm_policy=3"
```

Nota `transparent_hugepage`: ya en `99-euler.conf` solo `vm.` sysctls; el cmdline es redundante pero kernel lo lee antes que sysctl, mantener ambos.

### 4.3 `sysctl.d/99-euler.conf` — validar junto a iGPU

Actual ya óptimo para UMA:

```
vm.swappiness=180          # fuerza zram antes que reclaim GTT → evita stalls de alloc i915/amdgpu
vm.vfs_cache_pressure=75   # retiene dentry para /usr/share/mesa shaders
vm.watermark_scale_factor=125  # 12.5% watermark, menos kswapd wakeup durante scanout
vm.min_free_kbytes=67584   # 66 MB libres para alloc atómico de BO 4K-8K sin direct reclaim
```

No añadir `vm.compact_memory` — ya `compaction_proactiveness=0`. Para iGPU bajo `madvise`, no necesita `defrag`.

---

## 5. Paquetes Debian testing

### 5.1 Estado verificado `euler-minbase.list.chroot` (63 líneas, 2026-08-29)

```
OK  linux-image-amd64
OK  firmware-linux  firmware-iwlwifi  firmware-amd-graphics  firmware-misc-nonfree  firmware-realtek
OK  intel-microcode  amd64-microcode
OK  mesa-vulkan-drivers   # RADV + ANV + zink + lavapipe
OK  mesa-va-drivers       # radeonsi + iHD VA-API
OK  libgl1-mesa-dri  libglx-mesa0  wayland-protocols wayland-utils
OK  libdrm (implícito: libdrm2 2.4.122, libdrm-amdgpu1/intel1 via deps)
MISS vulkan-tools          # ← añadir
```

`mesa-vulkan-drivers` en Debian testing ya incluye `anv` (Intel) + `radv` (AMD) + `llvmpipe`; no hace falta `mesa-vulkan-drivers:amd64` explícito. `libdrm2` viene como dep de `mesa-vulkan-drivers` pero documentar no duele.

### 5.2 Diferencia `libdrm*` explícito vs implícito

Debian `libdrm` es split: `libdrm2` (core) + `libdrm-amdgpu1` + `libdrm-intel1` + `libdrm-radeon1`. `mesa-vulkan-drivers` Depends: `libdrm-amdgpu1 (>=2.4.122), libdrm-intel1` según buildd, por lo que **no** hace falta listarlos a mano; `mmdebstrap` los traera transitivamente. Si se quiere reproducibilidad 100% air-gap, listarlos no rompe `<500 MB` (+ 150 KB).

### 5.3 Adición recomendada mínima (quedando <500 MB)

Actualizar `euler-minbase.list.chroot` (+2 líneas, + ~1.2 MB):

```diff
 mesa-vulkan-drivers
 mesa-va-drivers
+vulkan-tools
+libva-drm2
 libgl1-mesa-dri
```

- `vulkan-tools` (340 KB): `vulkaninfo`, `vkcube` — imprescindible para validar `VK_KHR_external_memory_fd` + `VK_EXT_image_drm_format_modifier` disponibles en el host. Sin esto `validate-iso.sh` no puede chequear iGPU. Ya estaba marcado `if missing vulkan-tools add?` en la tarea.
- `libva-drm2` (35 KB): asegura `vainfo` + `mpv --hwdec=vaapi` sin pulling `libva-wayland2` transitiva completa (pero `libva-wayland2` viene con `mesa-va-drivers`).
- Opcional no-minbase (mantener fuera de ISO base, en `euler-desktop.list` futuro): `intel-gpu-tools` (2.1 MB, `intel_gpu_top`), `radeontop` (38 KB), `libdrm-tests` para bench.

**No** añadir `libva-vdpau-driver`, `vdpau-driver-all` — duplican VA-API con indirection innecesaria; iGPU usa VA-API nativo.

Validación post-add:

```bash
grep -E "^(mesa-vulkan|mesa-va|vulkan-tools|libva-drm|libgl1)" config/euler/package-lists/euler-minbase.list.chroot
mmdebstrap --dry-run 2>&1 | grep -E "vulkan|libdrm"
vulkaninfo --summary | grep -E "driverID|GPU|Heap"
vainfo 2>&1 | head -20
```

### 5.4 Firmware — ya cubierta

`firmware-linux` meta + `firmware-amd-graphics` (Navi/RDNA3 780M) + `firmware-misc-nonfree` (Xe/Arc GuC/HuC/DMC) + `intel-microcode` + `amd64-microcode`. Verificar `dmesg | grep -i "firmware.*guc\|huc.*loaded"` tras añadir `enable_guc=3`.

---

## 6. Benchmark targets — `<500 MB idle` y compositor `<50 MB`, `memfd` `<50 fd`

### 6.1 Presupuesto RAM (`validate-iso.sh` ya implementa)

```
500 MB idle = kernel 85 + systemd 40 + pipewire 15 + NM/iwd 15 + sway/euler-comp 35 + buffers scanout 17 + buffers shm/dma-buf 17 + slack 271
```

Medición normativa (dentro de VM/live con 8 GB):

```bash
free | awk '/Mem:/ {print $3}'   # <500000 KB requerido
smem -t -k | tail -1
cat /proc/meminfo | grep -E "MemAvailable|Slab|SReclaimable|Shmem"
zramctl && cat /proc/swaps        # zram0 4G prio 100 + zram1 /tmp 512M
compsize / && btrfs filesystem usage /
```

Si `used >500k`, sospechosos: `tracker-miner`, `cups`, `ModemManager`, `colord`, `avahi` (ya mask en `system-preset/70-euler.preset`), o `journald Storage=persistent` (ya `volatile 32M`).

### 6.2 Compositor `<50 MB` — vs `cosmic-comp` regresiones

Bug `cosmic-comp#2073` memfd leak: cada frame creaba `memfd_create("cosmic-comp:shm")` sin `close`, RSS crecía 2 MB/min, `ls /proc/$(pgrep cosmic-comp)/fd | grep memfd | wc -l` >500 en 1h. Fix: `shm_open` + `lifecycle` con `FrameLifecycle` drop.

Target Euler:

| Métrica | Límite | Comando |
|---|---|---|
| `euler-comp` RSS | <50 MB post-login idle, 1 output 1080p | `ps -o rss= -p $(pgrep euler-comp)` |
| `euler-comp` memfd | <50 fd (steady) | `ls -l /proc/$(pgrep euler-comp)/fd | grep -c memfd` |
| memfd leak slope | 0 fd / 10 min | `watch -n 60 'ls /proc/$(pgrep euler-comp)/fd | wc -l'` |
| CPU wakeups | <100 /s idle | `powertop --csv` o `perf stat -e power/energy-pkg/` |
| Frame jitter | <1 ms stddev 60 Hz | `weston-presentation-shm` o `smithay` presentation-time |

`validate-iso.sh` ya:

```bash
if pgrep cosmic-comp >/dev/null 2>&1; then
    leak=$(ls /proc/"$(pgrep cosmic-comp)"/fd 2>/dev/null | grep -c "memfd" || true)
    [[ "$leak" -lt 50 ]] || bad "memfd leak $leak >50 — parche 2073 pendiente"
fi
```

Adaptar a `euler-comp` cuando exista (ver §7).

### 6.3 Herramientas bench iGPU

```bash
# Vulkan sintético
vkcube --c 1000 & sleep 2; ps -o rss= -p $!
vulkaninfo | grep -E "HeapBudget|HeapUsage" # VK_EXT_memory_budget

# VA-API decode (780M/UHD)
mpv --hwdec=vaapi --vo=gpu-next --gpu-context=wayland big_buck_bunny_1080p_h264.mov --msg-level=vo=v

# DRM scan
intel_gpu_top -o - | head   # si intel
radeontop -d - -l 1

# Wayland frame
WAYLAND_DEBUG=1 weston-simple-egl 2>&1 | grep -c frame

# Soak test 10 min
timeout 600 bash -c 'while true; do vkcube --c 100 >/dev/null 2>&1; sleep 1; free | awk "/Mem:/{print \$3}"; done' | awk '{if($1>500000) exit 1}'
```

### 6.4 Métricas softbuffer vs wgpu (medidas `cosmic-panel#596` + `grafting` demos)

- `softbuffer` applet 100×100 px: 0.4 MB SHM + 0.2 MB RSS, 3 ms blit @60 Hz en `x86-64-v3`.
- `wgpu` applet 400×300: 2.1 MB DMA-BUF import + 1.8 MB wgpu `Device` + 0.5 MB `naga` cache, 0.4 ms render.
- Compositor fullscreen: `softbuffer` 7.9 MB copy por frame (visible en `powertop` wakeups), `wgpu` 0 copy si `drmModePageFlip` con DMA-BUF scanout.

---

## 7. Roadmap — `euler-comp` en Rust con `wgpu` + `smithay`

Solo cuando se integre fork Euler (entonces `euler-session` en `config/euler/package-lists`). Stack 100% Rust, sin C++.

### 7.1 Arquitectura propuesta

```
euler-comp (binary, <50 MB RSS)
 ├─ smithay 0.5+ (Wayland compositor, DRM/KMS, libinput, xdg-shell, linux-dmabuf-v1)
 │   ├─ Backend DRM: GbmAllocator (ash-based) → GBM BO → dma_buf_fd
 │   └─ Renderer trait
 ├─ wgpu 29 + wgpu-hal (Vulkan guest via lamco-wgpu bridge)
 │   ├─ HostWgpuContext (VK_EXT_image_drm_format_modifier) → import dmabuf → texture
 │   └─ naga WGSL shaders (blur, shadows, rounded corners) — cache en /var/cache/euler-comp
 ├─ softbuffer applets (panel, workspaces) — vía smithay `SoftBuffer` fallback
 └─ calloop runtime (no tokio en hot path; tokio solo para IPC)
```

Por qué `lamco-wgpu::WgpuBridge` y no `wgpu` standalone: Smithay ya posee `Vulkan instance` + `Queue`; duplicar `ash::Instance` rompe `VK_KHR_external_memory` (fd solo válido dentro de mismo `VkDevice`). Bridge comparte `&smithay_device`.

### 7.2 Fases

**Fase 0 — Stubs sin compositor (actual, ya en `crates/euler-core`)**
- Mantener `<500 MB` sin compositor (solo `swaybg 3M` + `greetd 5M` sugerido en `tools/euler/README.md:88`).
- Añadir `crates/euler-comp` vacío con `Cargo.toml` + `smithay` dep opcional, CI `cargo clippy -- -D warnings` ya.

**Fase 1 — `euler-comp` minimal scanout (3-4 semanas, 1 dev)**
- `smithay::backend::drm::DrmCompositor` + `GbmAllocator`.
- Import `linux-dmabuf-v1` → `grafting::import_vulkan_external_image` → quad sampler.
- Output `drmModeAtomicCommit` con `allow_modeset`.
- Criterio: `vkcube` Wayland <50 MB, `weston-simple-egl` 60 fps sin tearing, `memfd <50`.

**Fase 2 — Efectos `wgpu` (2 semanas)**
- WGSL shaders: `blur 9-tap`, `shadow 4px`, `roundrect` — precompilado con `naga --pipeline`.
- `wgpu::RenderPass` offscreen → `TextureUses::RESOURCE` → scanout.
- Bench jitter <1 ms (`presentation-time-v1`).

**Fase 3 — Panel + applets `softbuffer` (1 semana)**
- `cosmic-panel` fork → `euler-panel` con `softbuffer` por defecto, `wgpu` feature para applets pesados.
- Evitar `WAYLAND_SOCKET` + `wgpu` sin `X_PRIVILEGED_WAYLAND_SOCKET` (issue #596 workaround: unset `WAYLAND_SOCKET` si `WGPU_BACKEND=vulkan`).

**Fase 4 — Fork COSMIC parcheado (cuando `euler-session` entre en `package-lists`)**
- Parches obligatorios antes de `<500 MB` medir: `cosmic-comp#2073 memfd leak`, `cosmic-comp#2265 oomd`, `cosmic-panel#596 WAYLAND_SOCKET`.
- `config/euler/package-lists/euler-minbase.list.chroot` entonces añade `euler-comp` + `euler-panel`.

### 7.3 Dependencias Cargo (esqueleto)

```toml
# crates/euler-comp/Cargo.toml (no incluir aún, solo referencia)
[package]
name = "euler-comp"
version.workspace = true
edition.workspace = true
rust-version.workspace = true

[dependencies]
smithay = { version = "0.5", features = ["backend_drm", "backend_winit", "renderer_gles2"] }
wgpu = { version = "29", features = ["wgsl"] }
bytemuck = { version = "1.19", features = ["derive"] }
grafting = { version = "0.5", features = ["wgpu-29", "dmabuf"] } # o lamco-wgpu
softbuffer = { version = "0.4", optional = true }
ash = { version = "0.38", optional = true }
calloop = "0.13"
wayland-server = "0.32"
drm = "0.14"
gbm = "0.18"
input = "0.9"

[features]
default = ["wgpu-renderer"]
wgpu-renderer = ["dep:wgpu", "dep:grafting"]
ash-fallback = ["dep:ash"]
softbuffer-fallback = ["dep:softbuffer"]
```

Compila con `.cargo/config.toml` ya `target-cpu=x86-64-v3`; `RUSTFLAGS` aplica igual a `wgpu`/`naga`/`smithay`.

### 7.4 Criterios de integración

- `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` (AGENTS.md) pasa.
- `tools/euler/validate-iso.sh` extiende chequeo: `euler-comp` RSS + memfd.
- ISO `mksquashfs -fstime + SOURCE_DATE_EPOCH` reproducible, `-volume_date`, tamaño <1.2 GB (ISO) / <500 MB idle (boot).
- Documentar `VK_KHR_external_memory` + `VK_EXT_image_drm_format_modifier` como requires en `README.md` sección "Requisitos Vulkan".

---

## Apéndice A — Verificación rápida host Euler (copy-paste)

```bash
# 1. Paquetes
grep -E "^(mesa-vulkan|mesa-va|vulkan-tools|libva|libgl1)" config/euler/package-lists/euler-minbase.list.chroot

# 2. Grub
grep CMDLINE config/euler/includes.chroot/etc/default/grub
# esperado: i915.enable_guc=3 amdgpu.ppfeaturemask=0xffffffff amd_pstate=active ...

# 3. Sysctl
cat config/euler/includes.chroot/etc/sysctl.d/99-euler.conf
# vm.swappiness=180 vm.min_free_kbytes=67584 ...

# 4. Rust SIMD
cat .cargo/config.toml  # rustflags = ["-C", "target-cpu=x86-64-v3"]
cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings

# 5. Host Vulkan/VA-API (dentro de Euler)
vulkaninfo --summary | head -60
vainfo 2>&1 | head -20
glxinfo -B 2>&1 | grep renderer
dmesg | grep -E "GuC|HuC|ppfeaturemask|amd_pstate" | head

# 6. Bench idle
free | awk '/Mem:/{print $3}' # <500000
ps -o rss,comm -p $(pgrep -x euler-comp || pgrep -x sway || echo 1) 2>/dev/null
ls /proc/$(pgrep -x euler-comp 2>/dev/null || echo 1)/fd 2>/dev/null | wc -l
```

> **Instalador responsive (360px–1920px):** `crates/euler-installer/src/gui.rs:73` `terminal_width()` (respeta `$COLUMNS`, fallback 80) → `adaptive_cols(width)` (`gui.rs:79`: `<600→1 col`, `<1024→2`, `≥1024→3`) para `render_hardware_card` (`gui.rs:90`) y `render_codec_menu` (`gui.rs:118`); truncate con `…` en `crates/euler-installer/src/main.rs:196` `truncate_str` (`width.saturating_sub(30).max(10)` / `width/2`) y corte a `width` en menú (`gui.rs:128`). CLI espejo: `--hw-profile auto|intel|amd|generic|minimal`, `--codecs …|all`, `--detect-hardware` (`main.rs:96,105,93`). Ver `tools/euler/README.md` § Instalador.

## Apéndice B — Referencias

- `wgpu` docs & source: `gfx-rs/wgpu` `wgpu/src/api/buffer.rs` mappedAtCreation + `bytemuck` pattern, `wgpu-hal` ash backend, `naga` WGSL→SPIRV.
- `ash` 0.38: `MaikKlein/ash`, zero-cost `#[inline]` fn ptr, vk.xml generator.
- `vulkano` 0.35 + `vulkano-taskgraph`: migración gen2→declared DAG (Feb 2025).
- `softbuffer` 0.4: `rust-windowing/softbuffer` Tier1 Wayland/X11.
- DMA-BUF interop: `grafting` 0.5 `vulkan_dmabuf.rs` (`VK_KHR_external_memory_fd` + `VK_EXT_image_drm_format_modifier`), `lamco-wgpu` (wgpu guest de Smithay), `wgpu#2320` texture import, `wayland-drm` / `linux-dmabuf-v1`.
- Smithay `ash` GbmAllocator + `cosmic-comp#2055`, `smithay#928` wgpu renderer discussion, `cosmic-panel#596` WAYLAND_SOCKET wgpu break.
- Kernel params: `kernel.org/doc/html/latest/gpu/amdgpu/module-parameters.html` ppfeaturemask, `intel_uc.c` `enable_guc` defaults Gen12, `i915_scheduler RFC`, `amd_pstate` active.
- Euler `tools/euler/README.md`, `validate-iso.sh`, `package-lists/euler-minbase.list.chroot`, `.cargo/config.toml` x86-64-v3.
- Instalador responsive 360px–1920px: `crates/euler-installer/src/gui.rs:73` `terminal_width` → `gui.rs:79` `adaptive_cols` 1/2/3 (`<600`/` <1024`/`≥1024`) + `crates/euler-installer/src/main.rs:196` `truncate_str` y `gui.rs:128` corte a `width` (ver `tools/euler/README.md` § Instalador).

---
*Euler OS — 100% Rust iGPU stack. Generado 2026-08-29, valida con `cargo test --workspace`.*
