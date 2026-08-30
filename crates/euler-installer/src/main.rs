//! euler-installer bin — CLI que muestra plan (dry-run por defecto).
//! Nunca ejecuta comandos destructivos; el daemon es quien ejecuta con privilegios.

use euler_core::hw::HwProfile;
use euler_installer::{build_plan, InstallRequest};

fn parse_args(args: &[String]) -> Result<(InstallRequest, bool), String> {
    // --detect-hardware no requiere device/hostname/username ni password (solo reporte)
    if args.iter().any(|a| a == "--detect-hardware") {
        print_hardware_report_and_exit();
    }
    validate_args_len(args)?;
    let (encrypt, json_output, hw_profile, codecs, enable_bluetooth, enable_printer) =
        parse_flags(args)?;
    let pwd = get_password_env()?;
    let req = InstallRequest {
        device: args[1].clone(),
        hostname: args[2].clone(),
        username: args[3].clone(),
        password: pwd,
        encrypt,
        hw_profile,
        codecs,
        enable_bluetooth,
        enable_printer,
    };
    Ok((req, json_output))
}

fn validate_args_len(args: &[String]) -> Result<(), String> {
    if args.len() >= 4 {
        return Ok(());
    }
    Err(format!(
        "Uso: {} <device> <hostname> <username> [--no-encrypt] [--json] [--hw-profile <auto|intel|amd|generic|minimal>] [--codecs <h264,hevc,webp,heif|all>] [--enable-bluetooth] [--enable-printer] [--detect-hardware]\n\
         Password via env: EULER_PASSWORD='S3cur3!' {} /dev/sda euler euler\n\
         Ejemplo: EULER_PASSWORD='S3cur3!' {} /dev/sda euler euler --hw-profile auto --codecs h264,hevc --enable-bluetooth --enable-printer",
        args[0], args[0], args[0]
    ))
}

#[allow(clippy::type_complexity)]
fn parse_flags(
    args: &[String],
) -> Result<(bool, bool, Option<String>, Vec<String>, bool, bool), String> {
    let mut encrypt = true;
    let mut json_output = false;
    let mut hw_profile: Option<String> = None;
    let mut codecs: Vec<String> = Vec::new();
    let mut enable_bluetooth = false;
    let mut enable_printer = false;
    let mut i = 4;
    while i < args.len() {
        let flag = &args[i];
        // handle flags that consume next arg inside helper; helper may bump i
        handle_single_flag(
            flag,
            &args[0],
            &mut encrypt,
            &mut json_output,
            &mut hw_profile,
            &mut codecs,
            &mut enable_bluetooth,
            &mut enable_printer,
            args,
            &mut i,
        )?;
        i += 1;
    }
    Ok((
        encrypt,
        json_output,
        hw_profile,
        codecs,
        enable_bluetooth,
        enable_printer,
    ))
}

#[allow(clippy::too_many_arguments)]
fn handle_single_flag(
    flag: &str,
    prog: &str,
    encrypt: &mut bool,
    json_output: &mut bool,
    hw_profile: &mut Option<String>,
    codecs: &mut Vec<String>,
    enable_bluetooth: &mut bool,
    enable_printer: &mut bool,
    args: &[String],
    idx: &mut usize,
) -> Result<(), String> {
    match flag {
        "--password" => Err(
            "--password eliminado por seguridad (visible en ps). Usa EULER_PASSWORD env var: EULER_PASSWORD='...' euler-installer /dev/sda euler euler"
                .to_string(),
        ),
        "--no-encrypt" => {
            *encrypt = false;
            Ok(())
        }
        "--json" => {
            *json_output = true;
            Ok(())
        }
        "--dry-run" => Ok(()),
        "--enable-bluetooth" => {
            *enable_bluetooth = true;
            Ok(())
        }
        "--enable-printer" | "--printer" => {
            *enable_printer = true;
            Ok(())
        }
        "--detect-hardware" => {
            print_hardware_report_and_exit();
        }
        "--hw-profile" => {
            let next = args.get(*idx + 1).ok_or_else(|| {
                "--hw-profile requiere valor: auto|intel|amd|generic|minimal".to_string()
            })?;
            validate_hw_profile(next)?;
            *hw_profile = Some(next.clone());
            *idx += 1;
            Ok(())
        }
        "--codecs" => {
            let next = args
                .get(*idx + 1)
                .ok_or_else(|| "--codecs requiere valor: h264,hevc,webp,heif o \"all\"".to_string())?;
            *codecs = parse_codecs_value(next)?;
            *idx += 1;
            Ok(())
        }
        "--help" | "-h" => Err(help_text(prog)),
        other => Err(format!("arg desconocido: {other}")),
    }
}

fn validate_hw_profile(s: &str) -> Result<(), String> {
    match s {
        "auto" | "intel" | "amd" | "generic" | "minimal" => Ok(()),
        _ => Err(format!(
            "hw-profile inválido: {s} (válidos: auto|intel|amd|generic|minimal)"
        )),
    }
}

fn parse_codecs_value(s: &str) -> Result<Vec<String>, String> {
    if s.trim().is_empty() {
        return Err("--codecs valor vacío".to_string());
    }
    if s.trim() == "all" {
        // Expand to all known codec ids
        let all: Vec<String> = euler_core::codecs::CODECS
            .iter()
            .map(|c| c.id.to_string())
            .collect();
        return Ok(all);
    }
    let mut out = Vec::new();
    for part in s.split(',') {
        let id = part.trim();
        if id.is_empty() {
            continue;
        }
        // validate against known list, but allow unknown with warning? we error for strictness
        if !euler_core::codecs::validate_codec_id(id) {
            return Err(format!(
                "codec desconocido: {id} (válidos: h264,hevc,av1,vp9,webp,heif,avif,bluetooth,audio-extra o \"all\")"
            ));
        }
        if !out.contains(&id.to_string()) {
            out.push(id.to_string());
        }
    }
    if out.is_empty() {
        return Err("--codecs no contiene codecs válidos".to_string());
    }
    Ok(out)
}

fn help_text(prog: &str) -> String {
    format!(
        "Uso: {prog} <device> <hostname> <username> [opciones]  (EULER_PASSWORD env requerido)\n\
         Opciones:\n  \
           --no-encrypt                Desactivar cifrado LUKS\n  \
           --json                      Salida JSON del plan\n  \
           --hw-profile <perfil>       Perfil hardware: auto|intel|amd|generic|minimal\n  \
           --codecs <lista|all>        Codecs: h264,hevc,webp,heif,av1,vp9,avif,bluetooth,audio-extra o \"all\"\n  \
           --enable-bluetooth          Habilitar stack Bluetooth (bluez)\n  \
           --enable-printer            Habilitar impresora (CUPS)\n  \
           --detect-hardware           Detectar hardware y salir (reporte)\n  \
           --dry-run                   Alias dry-run (no-op)\n  \
           --help, -h                  Mostrar esta ayuda"
    )
}

fn print_hardware_report_and_exit() -> ! {
    let hw = HwProfile::detect();
    println!("Hardware detect:");
    println!(" CPU : {}", hw.cpu_vendor);
    println!(" RAM : {} MiB", hw.ram_mb);
    println!(" GPU : {}", hw.gpu);
    println!(" WIFI: {}", hw.wifi);
    println!(" BT  : {}", if hw.has_bluetooth { "sí" } else { "no" });
    println!(" NVMe: {}", if hw.has_nvme { "sí" } else { "no" });
    std::process::exit(0);
}

fn get_password_env() -> Result<String, String> {
    std::env::var("EULER_PASSWORD").map_err(|_| {
        "EULER_PASSWORD no está seteada. Por seguridad no se acepta --password (visible en ps).\n\
         Usa: EULER_PASSWORD='TuPass123!' euler-installer /dev/sda euler euler"
            .to_string()
    })
}

fn truncate_str(s: &str, max: usize) -> String {
    // delegate to theme::truncate_str — Scandinavian minimal, Unicode-safe, single glyph "…" for responsive truncate
    euler_installer::theme::truncate_str(s, max)
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (req, json_output) = match parse_args(&args) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };
    let plan = build_plan(&req)?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        let width: usize = std::env::var("COLUMNS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(80);
        println!(
            "Plan Euler para {} ({} pasos, encrypt={}):",
            req.device,
            plan.step_count(),
            req.encrypt
        );
        for (i, step) in plan.steps.iter().enumerate() {
            // no imprimir password si aparece (placeholder)
            let cmd_str = if step.command.join(" ").contains("__PASSWORD_PLACEHOLDER__") {
                "[COMANDO CON PASSWORD REDACTED]".to_string()
            } else {
                format!("{:?}", step.command)
            };
            let cmd_truncated = truncate_str(&cmd_str, width.saturating_sub(30).max(10));
            let desc_truncated = truncate_str(&step.description, width / 2);
            if width < 80 {
                println!(
                    "{:2}. [{}] {}",
                    i + 1,
                    step_description(&step.kind),
                    desc_truncated
                );
                println!("     -> {}", cmd_truncated);
            } else {
                println!(
                    "{:2}. [{}] {} -> {}",
                    i + 1,
                    step_description(&step.kind),
                    desc_truncated,
                    cmd_truncated
                );
            }
        }
        println!("\n[DRY-RUN] Ningún comando fue ejecutado. Use el daemon para ejecución real.");
    }
    Ok(())
}

fn step_description(kind: &euler_core::install::InstallStepKind) -> &'static str {
    match kind {
        euler_core::install::InstallStepKind::Partition => "PART",
        euler_core::install::InstallStepKind::FormatEfi => "EFI",
        euler_core::install::InstallStepKind::LuksFormat => "LUKS",
        euler_core::install::InstallStepKind::LuksOpen => "OPEN",
        euler_core::install::InstallStepKind::MkfsBtrfs => "BTRFS",
        euler_core::install::InstallStepKind::SubvolCreate => "SUBVOL",
        euler_core::install::InstallStepKind::Mount => "MOUNT",
        euler_core::install::InstallStepKind::UnpackSquashfs => "UNPACK",
        euler_core::install::InstallStepKind::HwDetect => "HWDET",
        euler_core::install::InstallStepKind::HwPackages => "HWPKG",
        euler_core::install::InstallStepKind::Fstab => "FSTAB",
        euler_core::install::InstallStepKind::Crypttab => "CRYPTTAB",
        euler_core::install::InstallStepKind::Users => "USER",
        euler_core::install::InstallStepKind::Bootloader => "GRUB",
        euler_core::install::InstallStepKind::Initramfs => "INITRD",
        euler_core::install::InstallStepKind::Done => "DONE",
    }
}
