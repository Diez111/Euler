//! euler-installer bin — CLI simple que muestra plan.

use euler_installer::{build_plan, InstallRequest};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Uso: {} <device> <hostname> <username>", args[0]);
        eprintln!("Ejemplo: {} /dev/sda euler euler", args[0]);
        std::process::exit(2);
    }
    let req = InstallRequest {
        device: args[1].clone(),
        hostname: args[2].clone(),
        username: args[3].clone(),
        password: "euler".to_string(),
        encrypt: true,
    };
    let plan = build_plan(&req)?;
    println!(
        "Plan Euler para {} ({} pasos):",
        req.device,
        plan.step_count()
    );
    for (i, step) in plan.steps.iter().enumerate() {
        println!(
            "{:2}. [{}] {} -> {:?}",
            i + 1,
            step_description(&step.kind),
            step.description,
            step.command
        );
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
        euler_core::install::InstallStepKind::Fstab => "FSTAB",
        euler_core::install::InstallStepKind::Crypttab => "CRYPTTAB",
        euler_core::install::InstallStepKind::Users => "USER",
        euler_core::install::InstallStepKind::Bootloader => "GRUB",
        euler_core::install::InstallStepKind::Initramfs => "INITRD",
        euler_core::install::InstallStepKind::Done => "DONE",
    }
}
