# msi-fan-control

A Tauri-based fan control utility for MSI laptops running Linux.

![MSI Fan Control](/src-tauri/icons/128x128.png)

![Status](https://img.shields.io/badge/status-MVP-blue)
![Platform](https://img.shields.io/badge/platform-Linux-green)
![License](https://img.shields.io/badge/license-MIT-yellow)

## Features

- **Cooler Boost** - Toggle maximum fan speed (~6000 RPM)
- **Real-time Monitoring** - CPU/GPU temperatures and fan speeds
- **Secure Architecture** - Privileged sidecar pattern with pkexec
- **Modern UI** - Dark theme with gradient accents

## Premium Features (Coming Soon)

- **Custom Fan Curves** - Set your own fan speed profiles
- **Auto-Profile Switching** - Change profiles based on power source (AC/Battery)
- **Historical Graphs** - View temperature and fan speed history
- **System Tray Customization** - More options for the tray icon
- **Multi-Model Support** - Support for more MSI laptop models

## Supported Models

- MSI GF65 Thin 10SDR (Main development target)
- *More models planned for future releases*

## Prerequisites

1. **Disable Secure Boot** in your BIOS settings.
2. Load the EC kernel module with write support:
   ```bash
   sudo modprobe ec_sys write_support=1
   ```

To make this persistent across reboots:
```bash
echo "ec_sys" | sudo tee /etc/modules-load.d/ec_sys.conf
echo "options ec_sys write_support=1" | sudo tee /etc/modprobe.d/ec_sys.conf
```

## Installation

### From .deb (Debian/Ubuntu)
```bash
sudo dpkg -i msi-fan-control_0.1.0_amd64.deb
```

### From AppImage
```bash
chmod +x msi-fan-control_0.1.0_amd64.AppImage
./msi-fan-control_0.1.0_amd64.AppImage
```

## How It Works

This application separates the UI (User Space) from the hardware control (Root Space) using a secure sidecar pattern.

```
┌─────────────────┐          ┌─────────────────┐
│   Svelte UI     │◄────────►│   Tauri Core    │
│  (User space)   │   IPC    │  (User space)   │
└─────────────────┘          └────────┬────────┘
                                      │ pkexec
                             ┌────────▼────────┐
                             │   msi-sidecar   │
                             │  (Root space)   │
                             └────────┬────────┘
                                      │ R/W
                             ┌────────▼────────┐
                             │  ec_sys module  │
                             │   /sys/kernel/  │
                             │  debug/ec/ec0/  │
                             └─────────────────┘
```

The GUI runs as a normal user. Only the small `msi-sidecar` binary runs as root, authorized via standard Linux Polkit.

## Development

### Requirements
- Node.js 18+
- Rust 1.70+
- Linux with GTK3 and WebKit2GTK

### Build from source
```bash
# Install dependencies
npm install

# Build sidecar binary
cd src-tauri/binaries/msi-sidecar
cargo build --release
cd ../../..
cp src-tauri/binaries/msi-sidecar/target/release/msi-sidecar \
   src-tauri/binaries/msi-sidecar-x86_64-unknown-linux-gnu

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## License

MIT
