#!/usr/bin/env bash
# Euler validate — checks post-boot <500MB y BTRFS
# Uso: sudo ./tools/euler/validate-iso.sh  (dentro de Euler live/instalado)

set -euo pipefail

fail=0
ok() { echo "  ✓ $*"; }
bad() { echo "  ✗ $*"; fail=1; }

echo "[euler validate]"

# RAM
used_kb=$(free | awk '/Mem:/ {print $3}')
echo "RAM usada: ${used_kb}KB"
if [[ "$used_kb" -lt 500000 ]]; then ok "RAM <500MB (${used_kb}KB)"; else bad "RAM >500MB (${used_kb}KB) — revisar servicios"; fi

# zram
if [[ -e /dev/zram0 ]]; then ok "zram presente"; zramctl 2>/dev/null | head -5 || true
else bad "zram no presente — systemctl enable systemd-zram-generator?"; fi

if sysctl vm.swappiness 2>/dev/null | grep -q "180"; then ok "swappiness 180"; else bad "swappiness !=180 — zram inútil"; fi

# BTRFS
if mount | grep -q "btrfs"; then
    ok "BTRFS montado"
    if mount | grep -q "compress=zstd:1"; then ok "compress=zstd:1"; else bad "sin compress=zstd:1"; fi
    if mount | grep -q "discard=async"; then ok "discard=async"; else bad "sin discard=async"; fi
    if mount | grep -q "space_cache=v2"; then ok "space_cache=v2"; else bad "sin space_cache=v2"; fi
    btrfs filesystem usage / 2>/dev/null | head -10 || true
    compsize / 2>/dev/null | head -5 || true
else
    bad "no BTRFS montado"
fi

# Scheduler
for dev in /sys/block/nvme*/queue/scheduler /sys/block/sda/queue/scheduler; do
    [[ -e "$dev" ]] && echo "  scheduler $dev: $(cat "$dev")"
done

# Servicios
if systemctl is-enabled systemd-oomd >/dev/null 2>&1; then ok "systemd-oomd enabled"; else bad "systemd-oomd no enabled"; fi
if journalctl --no-pager -u systemd-journald 2>/dev/null | head -1 >/dev/null; then
    if grep -q "Storage=volatile" /etc/systemd/journald.conf 2>/dev/null; then ok "journald volatile"; else bad "journald no volatile"; fi
fi

# memfd leak check
if pgrep cosmic-comp >/dev/null 2>&1; then
    leak=$(ls /proc/"$(pgrep cosmic-comp)"/fd 2>/dev/null | grep -c "memfd" || true)
    echo "  memfd count cosmic-comp: $leak"
    if [[ "$leak" -lt 50 ]]; then ok "memfd <50"; else bad "memfd leak $leak >50 — parche 2073 pendiente"; fi
fi

echo ""
if [[ $fail -eq 0 ]]; then
    echo "[ok] Euler validación pasó"
else
    echo "[fail] Euler validación falló — ver arriba"
fi
exit $fail
