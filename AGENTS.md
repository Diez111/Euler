# Euler OS — AGENTS.md

Workspace Rust 2021 `rust-version = 1.88`. Antes de declarar tarea completa:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Build ISO:
```bash
sudo ./tools/euler/mkiso.sh --release 2026.09.01
bash tools/euler/validate-iso.sh  # <500MB, BTRFS, zram, oomd
```

Fork COSMIC separado: `/tmp/cosmic-epoch` → `Euler-COSMIC` (cuando se integre, será `cosmic-session` en `config/euler/package-lists`).

Separado de Grafito: este repo es standalone, Grafito quedó en `/home/diez/Grafito` sin `crates/euler-*`.
