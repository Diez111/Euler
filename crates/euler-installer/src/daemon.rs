//! euler-installer-daemon — daemon privilegiado (polkit) que ejecuta plan.
//! Comunica vía stdin/stdout JSON simple (futuro: zbus D-Bus org.euler.installer).
//! Requiere root + polkit; valida todo input con `euler-core::validate`.
//! Refactorizado: main ≤15 complejidad via helpers enfocados.

use euler_installer::{build_plan, InstallRequest, InstallStatus};
use std::io::{BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;
use zeroize::Zeroize;

const MAX_INPUT_BYTES: usize = 64 * 1024;
const STEP_TIMEOUT: Duration = Duration::from_secs(600);

fn is_root() -> bool {
    // SAFETY: geteuid is thread-safe
    unsafe { libc::geteuid() == 0 }
}

// ——— helpers de emit ———

fn emit_json(status: &InstallStatus) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string(status)?);
    std::io::stdout().flush()?;
    Ok(())
}

fn emit_failed(msg: impl Into<String>) -> anyhow::Result<()> {
    emit_json(&InstallStatus::Failed(msg.into()))
}

fn emit_running(step: usize, total: usize, message: String) -> anyhow::Result<()> {
    emit_json(&InstallStatus::Running {
        step,
        total,
        message,
    })
}

// ——— helpers de input ———

fn read_limited_input() -> anyhow::Result<Vec<u8>> {
    let stdin = std::io::stdin();
    let mut reader = BufReader::with_capacity(8192, stdin.lock());
    let mut buf = Vec::with_capacity(8192);
    // +1 para detectar overflow
    let mut limited = (&mut reader).take(MAX_INPUT_BYTES as u64 + 1);
    limited.read_to_end(&mut buf)?;
    if buf.len() > MAX_INPUT_BYTES {
        anyhow::bail!("input demasiado grande (max 64KB)");
    }
    Ok(buf)
}

fn parse_request(buf: &[u8]) -> Result<InstallRequest, String> {
    serde_json::from_slice(buf).map_err(|e| format!("JSON inválido: {e}"))
}

// ——— helpers de clasificación de comandos ———

#[inline]
fn is_luks_format(cmd: &[String]) -> bool {
    cmd.iter().any(|c| c == "luksFormat")
}

#[inline]
fn is_luks_open(cmd: &[String]) -> bool {
    cmd.first().map(|s| s == "cryptsetup").unwrap_or(false)
        && cmd.iter().any(|c| c == "luks-euler")
        && cmd.iter().any(|c| c == "open")
}

#[inline]
fn is_chpasswd(cmd: &[String]) -> bool {
    cmd.iter().any(|c| c == "chpasswd") && cmd.iter().any(|c| c == "systemd-nspawn")
}

#[inline]
fn needs_legacy_password(cmd: &[String]) -> bool {
    cmd.join(" ").contains("__PASSWORD_PLACEHOLDER__")
}

// ——— helpers de spawn ———

fn build_spawn_command(
    cmd: &[String],
    password: &str,
    with_stdin: bool,
) -> anyhow::Result<Command> {
    let mut spawn_cmd = if needs_legacy_password(cmd) {
        let mut replaced = cmd.to_vec();
        for part in &mut replaced {
            if part.contains("__PASSWORD_PLACEHOLDER__") {
                *part = part.replace("__PASSWORD_PLACEHOLDER__", password);
            }
        }
        let mut c = Command::new(&replaced[0]);
        c.args(&replaced[1..]);
        c
    } else {
        let mut c = Command::new(&cmd[0]);
        c.args(&cmd[1..]);
        c
    };
    if with_stdin {
        spawn_cmd.stdin(Stdio::piped());
    } else {
        spawn_cmd.stdin(Stdio::null());
    }
    // Evitar deadlock pipe: no usar piped sin drenar; null evita bloqueo por buffer lleno.
    spawn_cmd.stdout(Stdio::null());
    spawn_cmd.stderr(Stdio::null());
    Ok(spawn_cmd)
}

fn pipe_secret(child: &mut Child, password: &str, username: &str, luks: bool, chpasswd: bool) {
    if luks {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(password.as_bytes());
            let _ = stdin.write_all(b"\n");
        }
    } else if chpasswd {
        if let Some(mut stdin) = child.stdin.take() {
            let mut creds = format!("{}:{}\n", username, password);
            let _ = stdin.write_all(creds.as_bytes());
            creds.zeroize();
        }
    }
}

fn wait_with_timeout(
    mut child: Child,
    idx: usize,
    description: &str,
) -> anyhow::Result<Option<String>> {
    match child.wait_timeout(STEP_TIMEOUT) {
        Ok(Some(status)) => {
            if status.success() {
                Ok(None)
            } else {
                Ok(Some(format!("proceso salió con status {}", status)))
            }
        }
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            emit_failed(format!(
                "paso {} timeout después de 600s: {}",
                idx + 1,
                description
            ))?;
            // Señalamos timeout como fallo ya emitido; caller debe retornar
            anyhow::bail!("timeout");
        }
        Err(e) => Ok(Some(format!("paso {} wait error: {}", idx + 1, e))),
    }
}

fn execute_step(
    cmd: &[String],
    password: &str,
    username: &str,
    idx: usize,
    description: &str,
) -> anyhow::Result<bool> {
    let luks_format = is_luks_format(cmd);
    let luks_open = is_luks_open(cmd);
    let chpasswd = is_chpasswd(cmd);
    let with_stdin = luks_format || luks_open || chpasswd;

    let mut spawn_cmd = build_spawn_command(cmd, password, with_stdin)?;
    let mut child = match spawn_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            emit_failed(format!("paso {} no pudo spawn: {}", idx + 1, e))?;
            return Ok(false);
        }
    };

    pipe_secret(
        &mut child,
        password,
        username,
        luks_format || luks_open,
        chpasswd,
    );

    let out = match wait_with_timeout(child, idx, description) {
        Ok(v) => v,
        Err(_) => return Ok(false), // timeout ya emitido
    };

    if let Some(err) = out {
        let truncated: String = err.chars().take(500).collect();
        emit_failed(format!("paso {} falló: {}", idx + 1, truncated))?;
        return Ok(false);
    }
    Ok(true)
}

// ——— main orquestador ———

fn handle_root_check() -> anyhow::Result<bool> {
    let do_exec = std::env::var("EULER_DAEMON_EXEC").as_deref() == Ok("1");
    if do_exec && !is_root() {
        emit_failed("daemon requiere root (polkit) para ejecución privilegiada")?;
        std::process::exit(1);
    }
    Ok(do_exec)
}

fn main() -> anyhow::Result<()> {
    let do_exec = handle_root_check()?;

    let mut input_buf = match read_limited_input() {
        Ok(b) => b,
        Err(e) => {
            emit_failed(e.to_string())?;
            return Ok(());
        }
    };

    if input_buf.iter().all(|b| b.is_ascii_whitespace()) {
        emit_failed("esperaba InstallRequest JSON en stdin")?;
        return Ok(());
    }

    let mut req: InstallRequest = match parse_request(&input_buf) {
        Ok(r) => r,
        Err(e) => {
            emit_failed(e)?;
            return Ok(());
        }
    };
    input_buf.zeroize();

    let hw = euler_core::hw::HwProfile::detect();
    let _ = emit_running(
        0,
        1,
        format!(
            "Detectando hardware: {:?} GPU={} RAM={}MB cpu={}",
            hw, hw.gpu, hw.ram_mb, hw.cpu_vendor
        ),
    );

    let plan = match build_plan(&req) {
        Ok(p) => p,
        Err(e) => {
            emit_failed(e.to_string())?;
            return Ok(());
        }
    };

    let total = plan.step_count();
    let mut password = req.password.clone();
    let username = req.username.clone();

    for (idx, step) in plan.steps.iter().enumerate() {
        emit_running(idx + 1, total, step.description.clone())?;

        if do_exec && !step.command.is_empty() {
            let ok = execute_step(&step.command, &password, &username, idx, &step.description)?;
            if !ok {
                password.zeroize();
                req.password.zeroize();
                return Ok(());
            }
        }
    }

    password.zeroize();
    req.password.zeroize();

    emit_json(&InstallStatus::Success)?;
    Ok(())
}
