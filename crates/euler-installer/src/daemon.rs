//! euler-installer-daemon — daemon privilegiado (polkit) que ejecuta plan.
//! Comunica vía stdin/stdout JSON simple (futuro: zbus D-Bus org.euler.installer).

use euler_installer::{build_plan, InstallRequest, InstallStatus};

fn main() -> anyhow::Result<()> {
    // Protocolo simple: lee InstallRequest JSON de stdin, emite InstallStatus JSON por línea
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
    if input.trim().is_empty() {
        eprintln!("daemon: esperando InstallRequest JSON en stdin");
        return Ok(());
    }
    let req: InstallRequest = serde_json::from_str(&input)?;
    let plan = match build_plan(&req) {
        Ok(p) => p,
        Err(e) => {
            let st = InstallStatus::Failed(e.to_string());
            println!("{}", serde_json::to_string(&st)?);
            return Ok(());
        }
    };

    let total = plan.step_count();
    for (idx, step) in plan.steps.iter().enumerate() {
        let status = InstallStatus::Running {
            step: idx + 1,
            total,
            message: step.description.clone(),
        };
        println!("{}", serde_json::to_string(&status)?);
        // Ejecución real: descomentar cuando se corra como root
        // if !step.command.is_empty() {
        //     let out = std::process::Command::new(&step.command[0])
        //         .args(&step.command[1..])
        //         .output()?;
        //     if !out.status.success() {
        //         let err = String::from_utf8_lossy(&out.stderr).to_string();
        //         let st = InstallStatus::Failed(format!("paso {} falló: {}", idx + 1, err));
        //         println!("{}", serde_json::to_string(&st)?);
        //         return Ok(());
        //     }
        // }
        // Simulación: no ejecuta comandos destructivos en build
        let _ = &step.command;
    }

    let done = InstallStatus::Success;
    println!("{}", serde_json::to_string(&done)?);
    Ok(())
}
