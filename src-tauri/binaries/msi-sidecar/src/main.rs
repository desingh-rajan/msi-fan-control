//! MSI EC Sidecar - Privileged binary for EC register access
//!
//! This binary runs with root privileges via pkexec and handles
//! all Embedded Controller I/O operations.

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::Command as ProcessCommand;

const EC_IO_PATH: &str = "/sys/kernel/debug/ec/ec0/io";

// Register offsets from MSI EC documentation & MControlCenter
const REG_CPU_TEMP: u64 = 0x68;
const REG_GPU_TEMP: u64 = 0x80;

const REG_COOLER_BOOST: u64 = 0x98;
const COOLER_BOOST_BIT: u8 = 0x80; // Bit 7

// Fan 1 (CPU) candidates
const REG_FAN1_RPM_L_0XC9: u64 = 0xC9;
const REG_FAN1_RPM_H_0XC9: u64 = 0xC8;
const REG_FAN1_RPM_L_0XCD: u64 = 0xCD; // MControlCenter prefers CD if non-zero
const REG_FAN1_RPM_H_0XCD: u64 = 0xCC;

// Fan 2 (GPU)
const REG_FAN2_RPM_L: u64 = 0xCB;
const REG_FAN2_RPM_H: u64 = 0xCA;

// Fan mode control (Advanced fan control)
const REG_FAN_MODE_0XD4: u64 = 0xD4;
const REG_FAN_MODE_0XF4: u64 = 0xF4;
const FAN_MODE_AUTO: u8 = 0x0D;
const FAN_MODE_SILENT: u8 = 0x1D;
const FAN_MODE_BASIC: u8 = 0x4D;
const FAN_MODE_ADVANCED: u8 = 0x8D;

// Fan 1 (CPU) speed curve - 7 speed points (0x72-0x78)
const REG_FAN1_SPEED_START: u64 = 0x72;
// Fan 2 (GPU) speed curve - 7 speed points (0x8A-0x90)
const REG_FAN2_SPEED_START: u64 = 0x8A;
const FAN_SPEED_POINTS: u64 = 7;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "data")]
enum Command {
    #[serde(rename = "get_status")]
    GetStatus,
    #[serde(rename = "set_cooler_boost")]
    SetCoolerBoost { enabled: bool },
    #[serde(rename = "set_fan_speed")]
    SetFanSpeed { percent: u8 },
    #[serde(rename = "set_fan_mode")]
    SetFanMode { mode: String },
    #[serde(rename = "exit")]
    Exit,
}

#[derive(Debug, Serialize)]
struct Status {
    cpu_temp: u8,
    gpu_temp: u8,
    fan1_rpm: u32,
    fan2_rpm: u32,
    cooler_boost: bool,
    fan_mode: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum Response {
    #[serde(rename = "status")]
    Status(Status),
    #[serde(rename = "ok")]
    Ok { message: String },
    #[serde(rename = "error")]
    Error { message: String },
}

fn setup_ec_module() {
    // 1. Check if module is loaded by checking file existence
    if !Path::new(EC_IO_PATH).exists() {
        eprintln!("EC module not loaded. Attempting to load...");
        let status = ProcessCommand::new("modprobe")
            .arg("ec_sys")
            .arg("write_support=1")
            .status();

        match status {
            Ok(s) if s.success() => eprintln!("Successfully loaded ec_sys"),
            _ => eprintln!("Failed to load ec_sys. Cooler Boost might fail."),
        }
    }

    // 2. Setup Persistence (Best Effort)
    // /etc/modules-load.d/ec_sys.conf
    let load_conf = Path::new("/etc/modules-load.d/ec_sys.conf");
    if !load_conf.exists() {
        if let Ok(mut f) = OpenOptions::new().create(true).truncate(true).write(true).open(load_conf) {
            let _ = writeln!(f, "ec_sys");
            eprintln!("Created persistence: {:?}", load_conf);
        }
    }

    // /etc/modprobe.d/ec_sys.conf
    let modprobe_conf = Path::new("/etc/modprobe.d/ec_sys.conf");
    if !modprobe_conf.exists() {
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(modprobe_conf)
        {
            let _ = writeln!(f, "options ec_sys write_support=1");
            eprintln!("Created persistence: {:?}", modprobe_conf);
        }
    }
}

fn read_ec_snapshot() -> io::Result<Vec<u8>> {
    let mut file = File::open(EC_IO_PATH)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

fn write_ec_byte(offset: u64, value: u8) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).open(EC_IO_PATH)?;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&[value])?;
    file.flush()?;
    Ok(())
}

fn get_fan_rpm(buffer: &[u8], low_offset: usize, high_offset: usize) -> u32 {
    if high_offset >= buffer.len() || low_offset >= buffer.len() {
        return 0;
    }
    let low = buffer[low_offset] as u32;
    let high = buffer[high_offset] as u32;

    let value = (high << 8) | low;

    if value > 0 {
        470000 / value
    } else {
        0
    }
}

fn get_fan1_rpm(buffer: &[u8]) -> u32 {
    // Check 0xCD first
    let rpm_cd = get_fan_rpm(
        buffer,
        REG_FAN1_RPM_L_0XCD as usize,
        REG_FAN1_RPM_H_0XCD as usize,
    );
    if rpm_cd > 0 && rpm_cd < 10000 {
        return rpm_cd;
    }
    // Fallback to 0xC9
    get_fan_rpm(
        buffer,
        REG_FAN1_RPM_L_0XC9 as usize,
        REG_FAN1_RPM_H_0XC9 as usize,
    )
}

fn detect_fan_mode_address(buffer: &[u8]) -> u64 {
    let val_d4 = buffer.get(REG_FAN_MODE_0XD4 as usize).copied().unwrap_or(0);
    if val_d4 == FAN_MODE_AUTO
        || val_d4 == FAN_MODE_SILENT
        || val_d4 == FAN_MODE_BASIC
        || val_d4 == FAN_MODE_ADVANCED
    {
        return REG_FAN_MODE_0XD4;
    }
    REG_FAN_MODE_0XF4
}

fn get_fan_mode_string(buffer: &[u8]) -> String {
    let fan_mode_addr = detect_fan_mode_address(buffer);
    let mode_value = buffer.get(fan_mode_addr as usize).copied().unwrap_or(0);
    match mode_value {
        FAN_MODE_AUTO => "auto".to_string(),
        FAN_MODE_SILENT => "silent".to_string(),
        FAN_MODE_BASIC => "basic".to_string(),
        FAN_MODE_ADVANCED => "advanced".to_string(),
        _ => format!("unknown(0x{:02X})", mode_value),
    }
}

fn set_fan_speed_fixed(percent: u8) -> Result<(), String> {
    let buffer = read_ec_snapshot().map_err(|e| e.to_string())?;
    let fan_mode_addr = detect_fan_mode_address(&buffer);

    // 1. Enable Advanced mode
    write_ec_byte(fan_mode_addr, FAN_MODE_ADVANCED).map_err(|e| e.to_string())?;

    // 2. Set all 7 speed points to the same value for Fan 1 (CPU)
    for i in 0..FAN_SPEED_POINTS {
        write_ec_byte(REG_FAN1_SPEED_START + i, percent).map_err(|e| e.to_string())?;
    }

    // 3. Set all 7 speed points for Fan 2 (GPU)
    for i in 0..FAN_SPEED_POINTS {
        write_ec_byte(REG_FAN2_SPEED_START + i, percent).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn set_fan_mode(mode: &str) -> Result<(), String> {
    let buffer = read_ec_snapshot().map_err(|e| e.to_string())?;
    let fan_mode_addr = detect_fan_mode_address(&buffer);

    let mode_value = match mode {
        "auto" => FAN_MODE_AUTO,
        "silent" => FAN_MODE_SILENT,
        "basic" => FAN_MODE_BASIC,
        "advanced" => FAN_MODE_ADVANCED,
        _ => return Err(format!("Unknown mode: {}", mode)),
    };

    write_ec_byte(fan_mode_addr, mode_value).map_err(|e| e.to_string())
}

fn get_status() -> Result<Status, String> {
    let buffer = read_ec_snapshot().map_err(|e| format!("Failed to read EC: {}", e))?;

    // Safety check
    if buffer.len() < 0xFF {
        // Ensure we have enough data
        return Err(format!("EC buffer too small: {} bytes", buffer.len()));
    }

    let cpu_temp = buffer.get(REG_CPU_TEMP as usize).copied().unwrap_or(0);
    let gpu_temp = buffer.get(REG_GPU_TEMP as usize).copied().unwrap_or(0);

    let cooler_boost_byte = buffer.get(REG_COOLER_BOOST as usize).copied().unwrap_or(0);
    let cooler_boost = (cooler_boost_byte & COOLER_BOOST_BIT) != 0;

    let fan1_rpm = get_fan1_rpm(&buffer);
    let fan2_rpm = get_fan_rpm(&buffer, REG_FAN2_RPM_L as usize, REG_FAN2_RPM_H as usize);
    let fan_mode = get_fan_mode_string(&buffer);

    Ok(Status {
        cpu_temp,
        gpu_temp,
        fan1_rpm,
        fan2_rpm,
        cooler_boost,
        fan_mode,
    })
}

fn set_cooler_boost(enabled: bool) -> Result<(), String> {
    // Read current state first
    let buffer = read_ec_snapshot().map_err(|e| e.to_string())?;
    // Or just open and read single byte?? Snapshot is safer.
    let current = buffer
        .get(REG_COOLER_BOOST as usize)
        .copied()
        .ok_or("Cannot read cooler boost reg")?;

    let new_value = if enabled {
        current | COOLER_BOOST_BIT
    } else {
        current & !COOLER_BOOST_BIT
    };

    write_ec_byte(REG_COOLER_BOOST, new_value).map_err(|e| e.to_string())?;

    // Check verification? skipping for speed, relying on UI to poll
    Ok(())
}

fn send_response(response: &Response) {
    if let Ok(json) = serde_json::to_string(response) {
        println!("{}", json);
        // Flush to ensure the response is sent immediately
        let _ = std::io::stdout().flush();
    }
}

fn main() {
    setup_ec_module();

    // Send initial status
    match get_status() {
        Ok(status) => send_response(&Response::Status(status)),
        Err(e) => send_response(&Response::Error { message: e }),
    }

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line: String = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.is_empty() {
            continue;
        }

        let cmd: Command = match serde_json::from_str(&line) {
            Ok(c) => c,
            Err(e) => {
                send_response(&Response::Error {
                    message: format!("Invalid command: {}", e),
                });
                continue;
            }
        };

        match cmd {
            Command::GetStatus => match get_status() {
                Ok(status) => send_response(&Response::Status(status)),
                Err(e) => send_response(&Response::Error { message: e }),
            },
            Command::SetCoolerBoost { enabled } => match set_cooler_boost(enabled) {
                Ok(()) => {
                    send_response(&Response::Ok {
                        message: format!(
                            "Cooler Boost {}",
                            if enabled { "enabled" } else { "disabled" }
                        ),
                    });
                }
                Err(e) => send_response(&Response::Error { message: e }),
            },
            Command::SetFanSpeed { percent } => match set_fan_speed_fixed(percent) {
                Ok(()) => {
                    send_response(&Response::Ok {
                        message: format!("Fan speed set to {}%", percent),
                    });
                }
                Err(e) => send_response(&Response::Error { message: e }),
            },
            Command::SetFanMode { mode } => match set_fan_mode(&mode) {
                Ok(()) => {
                    send_response(&Response::Ok {
                        message: format!("Fan mode set to {}", mode),
                    });
                }
                Err(e) => send_response(&Response::Error { message: e }),
            },
            Command::Exit => {
                send_response(&Response::Ok {
                    message: "Goodbye".to_string(),
                });
                break;
            }
        }
    }
}
