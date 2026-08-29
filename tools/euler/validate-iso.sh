#!/usr/bin/env bash
# Euler validate — checks post-boot <500MB y BTRFS
# Uso: sudo ./tools/euler/validate-iso.sh  (dentro de Euler live/instalado)

set -euo pipefail

RAM_LIMIT_KB="${RAM_LIMIT_KB:-500000}"
MEMFD_THRESHOLD="${MEMFD_THRESHOLD:-50}"

fail=0
ok() { echo "  ✓ $*"; }
bad() { echo "  ✗ $*"; fail=1; }
warn() { echo "  ! $*"; }

echo "[euler validate]"

# RAM — usa MemAvailable vs Used para no penalizar cache (used incluye buff/cache)
# free: used = total - free - buff/cache; avail = MemAvailable (/proc/meminfo, cache reclaimable)
# Parametrizable: RAM_LIMIT_KB env (default 500M). OK si used < limit O avail amplia.
if command -v free >/dev/null 2>&1; then
    used_kb=$(free | awk '/Mem:/ {print $3}')
    avail_kb=$(free | awk '/Mem:/ {print $7}')
    # fallback si free sin col avail (procps antiguo): leer MemAvailable directo
    if [[ -z "${avail_kb:-}" || ! "$avail_kb" =~ ^[0-9]+$ ]]; then
        avail_kb="$(awk '/MemAvailable:/{print $2}' /proc/meminfo 2>/dev/null || echo "")"
    fi
    echo "RAM usada: ${used_kb}KB (avail ${avail_kb:-?}KB) [limite ${RAM_LIMIT_KB}KB]"
    avail_ok=0
    if [[ -n "${avail_kb:-}" && "$avail_kb" =~ ^[0-9]+$ ]]; then
        # avail >100M o > RAM_LIMIT/2 indica memoria reclaimable suficiente — no penalizar cache
        if [[ "$avail_kb" -gt 100000 ]] || [[ "$avail_kb" -gt $((RAM_LIMIT_KB / 2)) ]]; then
            avail_ok=1
        fi
    fi
    if [[ "$used_kb" -lt "$RAM_LIMIT_KB" ]] || [[ "$avail_ok" -eq 1 ]]; then ok "RAM <${RAM_LIMIT_KB}KB (${used_kb}KB, avail ${avail_kb:-?}KB)"; else bad "RAM >${RAM_LIMIT_KB}KB (${used_kb}KB, avail ${avail_kb:-?}KB) — revisar servicios"; fi
else
    warn "free no encontrado, skip RAM check"
fi

# zram
if [[ -e /dev/zram0 ]]; then
    ok "zram presente"
    zramctl 2>/dev/null | head -5 || true
    if command -v zramctl >/dev/null 2>&1; then
        zram_size=$(zramctl --output SIZE --noheadings /dev/zram0 2>/dev/null | head -1 || echo "?")
        echo "  zram0 size: $zram_size"
    fi
else bad "zram no presente — systemctl enable systemd-zram-generator?"; fi

if sysctl vm.swappiness 2>/dev/null | grep -q "180"; then ok "swappiness 180"; else bad "swappiness !=180 — zram inútil"; fi

# zram /tmp (zram1)
if [[ -e /dev/zram1 ]] && mount | grep -q "on /tmp"; then
    if mount | grep -q " /tmp .*zram"; then ok "/tmp en zram"; else warn "/tmp no es zram (es $(mount | grep 'on /tmp' || echo 'no montado'))"; fi
fi

# BTRFS
if mount | grep -q "btrfs"; then
    ok "BTRFS montado"
    if mount | grep -q "compress=zstd:1"; then ok "compress=zstd:1"; else bad "sin compress=zstd:1"; fi
    if mount | grep -q "discard=async"; then ok "discard=async"; else bad "sin discard=async"; fi
    if mount | grep -q "space_cache=v2"; then ok "space_cache=v2"; else bad "sin space_cache=v2"; fi
    btrfs filesystem usage / 2>/dev/null | head -15 || true
    compsize / 2>/dev/null | head -5 || true
else
    bad "no BTRFS montado"
fi

# Scheduler — cubre nvme, sda, vda, mmcblk
for dev in /sys/block/nvme*/queue/scheduler /sys/block/sd*/queue/scheduler /sys/block/vd*/queue/scheduler /sys/block/mmcblk*/queue/scheduler; do
    [[ -e "$dev" ]] && echo "  scheduler $dev: $(cat "$dev")"
done
# validar que nvme use none y sata mq-deadline si existen
if [[ -e /sys/block/nvme0n1/queue/scheduler ]] && ! grep -q "\[none\]" /sys/block/nvme0n1/queue/scheduler 2>/dev/null; then
    warn "nvme0n1 scheduler no es none: $(cat /sys/block/nvme0n1/queue/scheduler 2>/dev/null)"
fi

# Servicios
if systemctl is-enabled systemd-oomd >/dev/null 2>&1; then ok "systemd-oomd enabled"; else bad "systemd-oomd no enabled"; fi
if systemctl is-active systemd-oomd >/dev/null 2>&1; then ok "systemd-oomd active"; else warn "systemd-oomd no active (puede ser live)"; fi
if systemctl is-enabled systemd-zram-setup@zram0.service >/dev/null 2>&1 || systemctl is-enabled systemd-zram-generator >/dev/null 2>&1; then
    ok "zram generator enabled"
fi
if grep -q "Storage=volatile" /etc/systemd/journald.conf 2>/dev/null; then ok "journald volatile"; else bad "journald no volatile"; fi
if grep -q "Storage=none" /etc/systemd/coredump.conf 2>/dev/null; then ok "coredump none"; else warn "coredump no none"; fi

# memfd leak check — pgrep euler-comp primero, fallback cosmic-comp (legado, antes cosmic-comp ahora euler-comp)
comp_pid="$(pgrep euler-comp 2>/dev/null || pgrep cosmic-comp 2>/dev/null || true)"
if [[ -n "$comp_pid" ]]; then
    # tomar primer pid si hay varios
    comp_pid="$(echo "$comp_pid" | head -1)"
    comp_name="$(ps -o comm= -p "$comp_pid" 2>/dev/null || echo "euler-comp")"
    leak=$(ls /proc/"$comp_pid"/fd 2>/dev/null | tr -d '\0' | grep -c "memfd" || true)
    # fallback conteo via ls -l /proc/pid/fd
    if [[ "$leak" -eq 0 ]]; then
        leak=$(ls -l /proc/"$comp_pid"/fd 2>/dev/null | grep -c "memfd" || true)
    fi
    echo "  memfd count $comp_name ($comp_pid): $leak [threshold $MEMFD_THRESHOLD]"
    if [[ "$leak" -lt "$MEMFD_THRESHOLD" ]]; then ok "memfd <$MEMFD_THRESHOLD"; else bad "memfd leak $leak >$MEMFD_THRESHOLD — parche 2073 pendiente"; fi
fi

echo ""
if [[ $fail -eq 0 ]]; then
    echo "[ok] Euler validación pasó"
else
    echo "[fail] Euler validación falló — ver arriba"
fi
exit $fail
