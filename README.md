# MSI Fan Control

A Tauri-based fan control utility for MSI GF65 Thin 10SDR laptops running Linux.

![Status](https://img.shields.io/badge/status-MVP-blue)
![Platform](https://img.shields.io/badge/platform-Linux-green)
![License](https://img.shields.io/badge/license-MIT-yellow)

## Features

- 🚀 **Cooler Boost** - Toggle maximum fan speed (~6000 RPM)
- 📊 **Real-time Monitoring** - CPU/GPU temperatures and fan speeds
- 🔒 **Secure Architecture** - Privileged sidecar pattern with pkexec
- 🎨 **Modern UI** - Dark theme with gradient accents

## Prerequisites

1. **Disable Secure Boot** in your BIOS settings
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
sudo dpkg -i MSI\ Fan\ Control_0.1.0_amd64.deb
```

### From AppImage
```bash
chmod +x MSI\ Fan\ Control_0.1.0_amd64.AppImage
./MSI\ Fan\ Control_0.1.0_amd64.AppImage
```

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
cd src-tauri/binaries/msi-sidecar && cargo build --release && cd ../../..
cp src-tauri/binaries/msi-sidecar/target/release/msi-sidecar \
   src-tauri/binaries/msi-sidecar-x86_64-unknown-linux-gnu

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Architecture

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

## EC Register Map (MS-16W1)

| Register | Function | Access |
|----------|----------|--------|
| 0x68 | CPU Temperature | Read |
| 0x80 | GPU Temperature | Read |
| 0x71 | CPU Fan Speed | Read |
| 0x89 | GPU Fan Speed | Read |
| 0x98 | Cooler Boost (bit 7) | Read/Write |

## Security Notes

- The GUI runs as a normal user
- Only the minimal sidecar binary runs as root
- Uses standard Linux polkit for privilege escalation

## License

MIT
