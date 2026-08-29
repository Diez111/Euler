//! Codecs opcionales Euler — selección post-instalación.
//! Tablas const zero-copy siguiendo patrón `btrfs.rs` `EULER_SUBVOLS_DATA`:
//! - `CODECS` es `&'static [CodecOption]` — 0 heap, backing store const
//! - `find_codec` / `validate_codec_id` hacen lookup O(n) sobre slice estático
//! - `CodecSelection::packages` / `total_size_mb` iteran sin alloc extra salvo `Vec` resultado
//!
//! Optimización alloc zero-copy:
//! - `CodecOption.packages` es `&'static [&'static str]` — inline, sin Vec
//! - `CODECS` no clona; callers que necesitan owned usan `.to_string()` / `.to_vec()` local

use serde::{Deserialize, Serialize};

/// Grupo de codec — usado para agrupar UI y cálculo de tamaño por categoría.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodecGroup {
    Video,
    Image,
    Bluetooth,
    Audio,
}

/// Opción de codec individual — todo `'static` para zero-copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecOption {
    pub id: &'static str,
    pub label: &'static str,
    pub group: CodecGroup,
    pub size_mb: u32,
    pub packages: &'static [&'static str],
    pub description: &'static str,
}

/// Tabla const de codecs disponibles — backing store estático, 0 heap.
/// Patrón idéntico a `EULER_SUBVOLS_DATA` en `btrfs.rs`.
pub const CODECS: &[CodecOption] = &[
    CodecOption {
        id: "h264",
        label: "H.264 / AVC",
        group: CodecGroup::Video,
        size_mb: 80,
        packages: &["gstreamer1.0-libav", "gstreamer1.0-plugins-ugly"],
        description: "Decodificación H.264/AVC — compatibilidad máxima video web y cámaras",
    },
    CodecOption {
        id: "hevc",
        label: "HEVC / H.265",
        group: CodecGroup::Video,
        size_mb: 15,
        packages: &["gstreamer1.0-plugins-bad"],
        description: "Decodificación HEVC/H.265 — 4K y grabaciones modernas",
    },
    CodecOption {
        id: "av1",
        label: "AV1",
        group: CodecGroup::Video,
        size_mb: 20,
        packages: &["gstreamer1.0-libav"],
        description: "Decodificación AV1 — codec libre YouTube/Netflix",
    },
    CodecOption {
        id: "vp9",
        label: "VP9",
        group: CodecGroup::Video,
        size_mb: 5,
        packages: &["gstreamer1.0-plugins-good"],
        description: "Decodificación VP9 — YouTube y WebRTC",
    },
    CodecOption {
        id: "webp",
        label: "WebP",
        group: CodecGroup::Image,
        size_mb: 1,
        packages: &["webp-pixbuf-loader"],
        description: "Soporte imágenes WebP en visores y thumbnails",
    },
    CodecOption {
        id: "heif",
        label: "HEIF / HEIC",
        group: CodecGroup::Image,
        size_mb: 3,
        packages: &["heif-gdk-pixbuf"],
        description: "Soporte HEIF/HEIC — fotos iPhone",
    },
    CodecOption {
        id: "avif",
        label: "AVIF",
        group: CodecGroup::Image,
        size_mb: 2,
        packages: &["libavif-gdk-pixbuf", "libavif16"],
        description: "Soporte AVIF — imágenes AV1 de alta eficiencia",
    },
    CodecOption {
        id: "bluetooth",
        label: "Bluetooth",
        group: CodecGroup::Bluetooth,
        size_mb: 2,
        packages: &["bluez", "bluez-firmware"],
        description: "Stack Bluetooth — audio y periféricos inalámbricos",
    },
    CodecOption {
        id: "audio-extra",
        label: "Audio Extra",
        group: CodecGroup::Audio,
        size_mb: 10,
        packages: &["libavcodec-extra"],
        description: "Codecs audio propietarios extra — AAC, MP3 refinado",
    },
];

/// Selección de codecs del usuario — serializable para instalador / API.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodecSelection {
    pub video: Vec<String>,
    pub image: Vec<String>,
    pub bluetooth: bool,
}

impl CodecSelection {
    /// Suma total en MB de los codecs seleccionados.
    /// Deduplica ids repetidos para no contar doble.
    pub fn total_size_mb(&self) -> u32 {
        let mut total: u32 = 0;
        let mut seen: Vec<&str> = Vec::with_capacity(self.video.len() + self.image.len() + 1);

        for id in self.video.iter().chain(self.image.iter()) {
            if seen.contains(&id.as_str()) {
                continue;
            }
            seen.push(id.as_str());
            if let Some(opt) = find_codec(id) {
                total = total.saturating_add(opt.size_mb);
            }
        }

        if self.bluetooth {
            // evitar doble conteo si "bluetooth" también está en video/image
            let already = seen.contains(&"bluetooth");
            if !already {
                if let Some(opt) = find_codec("bluetooth") {
                    total = total.saturating_add(opt.size_mb);
                }
            }
        }

        // audio-extra no está en struct, pero si user lo mete en video/image lo contamos arriba;
        // no hay flag dedicado — se maneja via CODECS lookup genérico.
        total
    }

    /// Paquetes Debian necesarios para la selección, deduplicados preservando orden.
    pub fn packages(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        let mut seen_ids: Vec<String> = Vec::new();

        for id in self.video.iter().chain(self.image.iter()) {
            if seen_ids.contains(id) {
                continue;
            }
            seen_ids.push(id.clone());
            if let Some(opt) = find_codec(id) {
                for &pkg in opt.packages {
                    if !out.contains(&pkg) {
                        out.push(pkg);
                    }
                }
            }
        }

        if self.bluetooth {
            // evitar duplicar si ya seleccionado explícitamente
            let already = seen_ids.contains(&"bluetooth".to_string());
            if !already {
                if let Some(opt) = find_codec("bluetooth") {
                    for &pkg in opt.packages {
                        if !out.contains(&pkg) {
                            out.push(pkg);
                        }
                    }
                }
            }
        }

        out
    }
}

/// Valida si un id de codec existe en la tabla const.
#[inline]
pub fn validate_codec_id(id: &str) -> bool {
    find_codec(id).is_some()
}

/// Busca un codec por id en la tabla const — O(n), n=9, zero-copy retorno &'static.
#[inline]
pub fn find_codec(id: &str) -> Option<&'static CodecOption> {
    CODECS.iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_size_calculation() {
        let sel = CodecSelection {
            video: vec!["h264".to_string()],
            image: vec!["webp".to_string()],
            bluetooth: false,
        };
        assert_eq!(sel.total_size_mb(), 81); // 80 + 1

        let sel2 = CodecSelection {
            video: vec!["h264".to_string(), "av1".to_string()],
            image: vec!["heif".to_string()],
            bluetooth: true,
        };
        // h264 80 + av1 20 + heif 3 + bluetooth 2 = 105
        assert_eq!(sel2.total_size_mb(), 105);

        let empty = CodecSelection::default();
        assert_eq!(empty.total_size_mb(), 0);

        // dedup: h264 repetido no duplica tamaño
        let dup = CodecSelection {
            video: vec!["h264".to_string(), "h264".to_string()],
            image: vec![],
            bluetooth: false,
        };
        assert_eq!(dup.total_size_mb(), 80);
    }

    #[test]
    fn packages_collect_dedup() {
        let sel = CodecSelection {
            video: vec!["h264".to_string(), "av1".to_string()],
            image: vec![],
            bluetooth: false,
        };
        let pkgs = sel.packages();
        // h264: libav + ugly, av1: libav (dedup)
        assert!(pkgs.contains(&"gstreamer1.0-libav"));
        assert!(pkgs.contains(&"gstreamer1.0-plugins-ugly"));
        // libav debe aparecer solo una vez
        assert_eq!(
            pkgs.iter().filter(|&&p| p == "gstreamer1.0-libav").count(),
            1
        );
        assert_eq!(pkgs.len(), 2);

        let sel2 = CodecSelection {
            video: vec!["vp9".to_string()],
            image: vec!["webp".to_string(), "avif".to_string()],
            bluetooth: true,
        };
        let pkgs2 = sel2.packages();
        assert!(pkgs2.contains(&"gstreamer1.0-plugins-good"));
        assert!(pkgs2.contains(&"webp-pixbuf-loader"));
        assert!(pkgs2.contains(&"libavif-gdk-pixbuf"));
        assert!(pkgs2.contains(&"libavif16"));
        assert!(pkgs2.contains(&"bluez"));
        assert!(pkgs2.contains(&"bluez-firmware"));
        assert_eq!(pkgs2.len(), 6);

        let empty = CodecSelection::default();
        assert!(empty.packages().is_empty());
    }

    #[test]
    fn validate_and_find() {
        assert!(validate_codec_id("h264"));
        assert!(validate_codec_id("hevc"));
        assert!(validate_codec_id("av1"));
        assert!(validate_codec_id("vp9"));
        assert!(validate_codec_id("webp"));
        assert!(validate_codec_id("heif"));
        assert!(validate_codec_id("avif"));
        assert!(validate_codec_id("bluetooth"));
        assert!(validate_codec_id("audio-extra"));
        assert!(!validate_codec_id("invalid"));
        assert!(!validate_codec_id(""));
        assert!(!validate_codec_id("H264"));

        let c = find_codec("h264").unwrap();
        assert_eq!(c.id, "h264");
        assert_eq!(c.group, CodecGroup::Video);
        assert_eq!(c.size_mb, 80);

        assert!(find_codec("nope").is_none());
    }
}
