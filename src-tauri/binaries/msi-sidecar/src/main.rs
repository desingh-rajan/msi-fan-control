//! MSI EC Sidecar - Privileged binary for EC register access
//!
//! This binary runs with root privileges via pkexec and handles
//! all Embedded Controller I/O operations.

use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};

const EC_IO_PATH: &str = "/sys/kernel/debug/ec/ec0/io";

// Register offsets from MSI EC documentation
const REG_CPU_TEMP: u64 = 0x68;
const REG_GPU_TEMP: u64 = 0x80;
const REG_CPU_FAN_SPEED: u64 = 0x71;
const REG_GPU_FAN_SPEED: u64 = 0x89;
const REG_COOLER_BOOST: u64 = 0x98;
const COOLER_BOOST_BIT: u8 = 0x80; // Bit 7

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "data")]
enum Command {
    #[serde(rename = "get_status")]
    GetStatus,
    #[serde(rename = "set_cooler_boost")]
    SetCoolerBoost { enabled: bool },
    #[serde(rename = "exit")]
    Exit,
}

#[derive(Debug, Serialize)]
struct Status {
    cpu_temp: u8,
    gpu_temp: u8,
    cpu_fan_speed: u8,
    gpu_fan_speed: u8,
    cooler_boost: bool,
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

fn read_ec_byte(file: &mut File, offset: u64) -> io::Result<u8> {
    let mut buf = [0u8; 1];
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn write_ec_byte(file: &mut File, offset: u64, value: u8) -> io::Result<()> {
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&[value])?;
    file.flush()?;
    Ok(())
}

fn get_status(file: &mut File) -> Result<Status, String> {
    let cpu_temp = read_ec_byte(file, REG_CPU_TEMP).map_err(|e| e.to_string())?;
    let gpu_temp = read_ec_byte(file, REG_GPU_TEMP).map_err(|e| e.to_string())?;
    let cpu_fan_speed = read_ec_byte(file, REG_CPU_FAN_SPEED).map_err(|e| e.to_string())?;
    let gpu_fan_speed = read_ec_byte(file, REG_GPU_FAN_SPEED).map_err(|e| e.to_string())?;
    let cooler_boost_reg = read_ec_byte(file, REG_COOLER_BOOST).map_err(|e| e.to_string())?;
    let cooler_boost = (cooler_boost_reg & COOLER_BOOST_BIT) != 0;

    Ok(Status {
        cpu_temp,
        gpu_temp,
        cpu_fan_speed,
        gpu_fan_speed,
        cooler_boost,
    })
}

fn set_cooler_boost(file: &mut File, enabled: bool) -> Result<(), String> {
    // Read-Modify-Write pattern for safety
    let current = read_ec_byte(file, REG_COOLER_BOOST).map_err(|e| e.to_string())?;

    let new_value = if enabled {
        current | COOLER_BOOST_BIT // Set bit 7
    } else {
        current & !COOLER_BOOST_BIT // Clear bit 7
    };

    write_ec_byte(file, REG_COOLER_BOOST, new_value).map_err(|e| e.to_string())?;

    // Verify the write
    let verify = read_ec_byte(file, REG_COOLER_BOOST).map_err(|e| e.to_string())?;
    if (verify & COOLER_BOOST_BIT != 0) != enabled {
        return Err("Write verification failed".to_string());
    }

    Ok(())
}

fn send_response(response: &Response) {
    if let Ok(json) = serde_json::to_string(response) {
        println!("{}", json);
        // Flush to ensure the response is sent immediately
        let _ = std::io::stdout().flush();
    }
}

struct EcContext {
    file: File,
    write_enabled: bool,
}

fn main() {
    // Try to open EC interface - first with write, then read-only
    let (ec_file, write_enabled) = match OpenOptions::new().read(true).write(true).open(EC_IO_PATH)
    {
        Ok(f) => (f, true),
        Err(_) => {
            // Try read-only mode
            match OpenOptions::new().read(true).open(EC_IO_PATH) {
                Ok(f) => {
                    eprintln!(
                        "Warning: EC opened in read-only mode. Cooler Boost control disabled."
                    );
                    (f, false)
                }
                Err(e) => {
                    send_response(&Response::Error {
                        message: format!("Failed to open {}: {}. Is ec_sys loaded?", EC_IO_PATH, e),
                    });
                    std::process::exit(1);
                }
            }
        }
    };

    let mut ctx = EcContext {
        file: ec_file,
        write_enabled,
    };

    // Send initial status
    match get_status(&mut ctx.file) {
        Ok(status) => send_response(&Response::Status(status)),
        Err(e) => send_response(&Response::Error { message: e }),
    }

    // Main command loop - read from stdin
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
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
            Command::GetStatus => match get_status(&mut ctx.file) {
                Ok(status) => send_response(&Response::Status(status)),
                Err(e) => send_response(&Response::Error { message: e }),
            },
            Command::SetCoolerBoost { enabled } => {
                if !ctx.write_enabled {
                    send_response(&Response::Error {
                        message: "Write access disabled. Load ec_sys with write_support=1"
                            .to_string(),
                    });
                    continue;
                }
                match set_cooler_boost(&mut ctx.file, enabled) {
                    Ok(()) => {
                        send_response(&Response::Ok {
                            message: format!(
                                "Cooler Boost {}",
                                if enabled { "enabled" } else { "disabled" }
                            ),
                        });
                        // Also send updated status
                        if let Ok(status) = get_status(&mut ctx.file) {
                            send_response(&Response::Status(status));
                        }
                    }
                    Err(e) => send_response(&Response::Error { message: e }),
                }
            }
            Command::Exit => {
                send_response(&Response::Ok {
                    message: "Goodbye".to_string(),
                });
                break;
            }
        }
    }
}
