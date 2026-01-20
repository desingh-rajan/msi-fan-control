# Engineering Analysis: Tauri-Based Thermal Management Infrastructure for MSI GF65 Thin 10SDR (16W1EMS2) on Ubuntu Linux

## 1. Executive Overview and Architectural Scope

This research report provides a comprehensive engineering analysis and
implementation strategy for developing a custom fan control application for the
MSI GF65 Thin 10SDR laptop, specifically targeting the Ubuntu Linux operating
system. The MSI GF65 Thin 10SDR, characterized by its use of the MS-16W1
motherboard platform and the 16W1EMS2 Embedded Controller (EC) firmware,
presents a unique set of challenges for Linux users.

Unlike desktop platforms where standard ACPI implementations or Super I/O chips
(like the Nuvoton NCT6683) expose Pulse Width Modulation (PWM) controls via the
hwmon subsystem, MSI laptops utilize a proprietary EC memory map that requires
direct, privileged manipulation to access advanced thermal features like "Cooler
Boost" and custom fan curves.

The analysis is predicated on the user's requirement to bypass the limitations
of the default Linux thermal management, which often leaves fans spinning at
suboptimal speeds, leading to thermal throttling during high-load operations.
The proposed solution leverages the Tauri framework to create a secure,
high-performance desktop application.

### Key Technical Requirements

- **Register Identification**: Precise mapping of the EC registers for real-time
  fan speed monitoring (RPM/Duty Cycle) and the "Cooler Boost" toggle for the
  16W1EMS2 firmware.
- **Backend Logic**: Design of a Rust-based hardware abstraction layer capable
  of safely reading and writing to the EC via the Linux kernel's debug
  interfaces.
- **Privilege Escalation**: Implementation of a robust security model using the
  Sidecar design pattern and PolicyKit (pkexec) to manage root privileges
  without compromising the security of the GUI layer.
- **Hardware Safety**: Analysis of the specific electrical and logical behaviors
  of the MS-16W1 EC to prevent race conditions and hardware "bricking."

---

## 2. Hardware Platform Analysis: The MS-16W1 Ecosystem

### 2.1 Motherboard and Chipset Architecture

The MSI GF65 Thin 10SDR is built upon the MS-16W1 motherboard, a platform that
integrates the Intel HM470 chipset. The 10SDR model pairs this board with:

- 10th Generation Intel Core i7-10750H processor
- NVIDIA GeForce GTX 1660 Ti graphics card

The thermal design utilizes a shared heat pipe architecture, where the thermal
load of the CPU and GPU influences the fan curves of both cooling zones. The
Embedded Controller is the arbiter of this logic—typically an ENE KB9028 or
similar variant.

### 2.2 Embedded Controller (EC) Firmware: 16W1EMS2

The target firmware version is **16W1EMS2.103**. In the context of the msi-ec
project, this firmware belongs to the "Generation 1" configuration profile.

> **⚠️ WARNING**: MSI has iterated on their EC layout multiple times. Software
> written for a "Generation 2" device uses completely different addresses for
> fan control. Attempting to use a generic MSI fan control script without
> validating against the 16W1EMS2 memory map could result in writing control
> bits to reserved or voltage-regulation registers, potentially causing
> catastrophic hardware failure.

The EC exposes a 256-byte RAM window to the operating system at
`/sys/kernel/debug/ec/ec0/io`.

### 2.3 The "Zero RPM" Phenomenon

A recurring issue: the GF65 Thin series reports 0 RPM on the GPU fan during
low-load scenarios. This is not a sensor failure but a feature of NVIDIA Optimus
(hybrid graphics). When the discrete GPU is powered down (D3 Cold state), the EC
may mask the fan tachometer reading.

---

## 3. Embedded Controller Register Map

### Table 1: Comprehensive Register Map for MSI MS-16W1 (Firmware 16W1EMS2)

| Function         | Register Offset | Data Type | Access | Description                                    |
| ---------------- | --------------- | --------- | ------ | ---------------------------------------------- |
| CPU Temperature  | 0x68            | Int8      | RO     | Real-time CPU Package Temperature (°C)         |
| GPU Temperature  | 0x80            | Int8      | RO     | Real-time GPU Temperature (°C)                 |
| CPU Fan Speed    | 0x71            | Int8      | RO     | Real-time Fan Duty Cycle / Speed Level (0-150) |
| GPU Fan Speed    | 0x89            | Int8      | RO     | Real-time Fan Duty Cycle / Speed Level (0-150) |
| Cooler Boost     | 0x98            | Bitfield  | RW     | Bit 7: 1 = Enabled, 0 = Disabled               |
| Fan Control Mode | 0xF4            | Int8      | RW     | 0x0D=Auto, 0x8D=Advanced, 0x1D=Silent          |
| CPU Curve Temps  | 0x6A - 0x6F     | Array     | RW     | Thresholds for CPU Fan Curve                   |
| CPU Curve Speeds | 0x72 - 0x77     | Array     | RW     | Speeds for CPU Fan Curve                       |
| GPU Curve Temps  | 0x82 - 0x87     | Array     | RW     | Thresholds for GPU Fan Curve                   |
| GPU Curve Speeds | 0x8A - 0x8F     | Array     | RW     | Speeds for GPU Fan Curve                       |

### 3.1 Cooler Boost Control: Register 0x98

- **Target Bit**: Bit 7 (MSB)
- **Enable (ON)**: Set Bit 7 to 1
- **Disable (OFF)**: Set Bit 7 to 0

> **IMPORTANT**: Must use Read-Modify-Write (RMW) operation to preserve bits
> 0-6.

### 3.2 Fan Speed Monitoring

- **Duty Cycle (Power %)**: Registers `0x71` (CPU) and `0x89` (GPU). Range
  0-150.
- **Real-Time RPM (Tachometer)**:
  - **CPU**: `0xCC` (High Byte) + `0xCD` (Low Byte)
  - **GPU**: `0xCE` (High Byte) + `0xCF` (Low Byte)

To display true RPM, the application must read these 2-byte values instead of
estimating from the duty cycle.

### 3.3 Fan Control Mode: 0xF4

| Mode          | Value | Description                    |
| ------------- | ----- | ------------------------------ |
| Auto Mode     | 0x0D  | Default BIOS behavior          |
| Silent Mode   | 0x1D  | Quiet operation                |
| Basic Mode    | 0x4D  | Simple slider                  |
| Advanced Mode | 0x8D  | Enables programmable fan curve |

### 3.4 Advanced Fan Curve Registers

**CPU Curve:**

- Temperature Thresholds: 0x6A - 0x6F (6 bytes, integer °C)
- Fan Speeds: 0x72 - 0x77 (6 bytes, percentage/duty cycle)

**GPU Curve:**

- Temperature Thresholds: 0x82 - 0x87 (6 bytes, integer °C)
- Fan Speeds: 0x8A - 0x8F (6 bytes, percentage/duty cycle)

---

## 4. Linux Kernel Integration and Security Constraints

### 4.1 The ec_sys Kernel Module

The Linux kernel's `ec_sys` module exposes EC memory to userspace at
`/sys/kernel/debug/ec/ec0/io`.

- **Read Access**: Default behavior
- **Write Access**: Requires `write_support=1` parameter

```bash
sudo modprobe ec_sys write_support=1
```

### 4.2 The Secure Boot Barrier

When Secure Boot is enabled, the Linux kernel enters "Lockdown Mode" which
restricts direct hardware memory access. The `ec_sys` module will refuse to load
with `write_support=1`.

**Solution**: Disable Secure Boot in BIOS or sign the kernel module with a
Machine Owner Key (MOK).

### 4.3 Persistent Configuration

To make ec_sys load on boot:

```bash
echo "ec_sys" | sudo tee /etc/modules-load.d/ec_sys.conf
echo "options ec_sys write_support=1" | sudo tee /etc/modprobe.d/ec_sys.conf
```

---

## 5. Tauri Application Architecture: The Sidecar Pattern

### 5.1 Architectural Split

1. **Frontend (UI) & Core**: Runs as standard user, renders interface
2. **Privileged Sidecar**: Runs as root, performs EC I/O

### 5.2 Privilege Escalation: pkexec

The application uses `pkexec` to launch the sidecar with root privileges,
triggering the standard Linux authentication dialog.

### 5.3 Inter-Process Communication (IPC)

- **Command Channel (Core → Sidecar)**: JSON commands via stdin
- **Data Channel (Sidecar → Core)**: JSON status via stdout

---

## 6. Polkit Configuration for Passwordless Operation

### File: `/usr/share/polkit-1/actions/com.msi.fancontrol.policy`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="com.msi.fancontrol.sidecar">
    <description>Run MSI Fan Control Sidecar</description>
    <message>Authentication is required to control fan speeds</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>yes</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/usr/bin/msi-sidecar</annotate>
    <annotate key="org.freedesktop.policykit.exec.allow_gui">false</annotate>
  </action>
</policyconfig>
```

---

## 7. Development Roadmap

### Phase 1: Read-Only Verification ✅

- Validate EC access without risking hardware
- Read 0x71, 0x89, temperatures

### Phase 2: Write Control ✅

- Implement Cooler Boost (RMW on 0x98)
- Toggle and verify fans spin up

### Phase 3: Curve Implementation

- Full "Advanced Mode" control
- Write 6-point curve tables

### Phase 4: Packaging

- Create .deb installer
- Include Polkit policy files

---

## 9. Conclusion

The development of a Tauri-based fan control application for the MSI GF65 Thin
10SDR on Ubuntu is technically feasible. The primary barrier—the lack of
standard kernel drivers—is overcome by a userspace architecture that leverages
the `ec_sys` debugging interface.

By strictly adhering to the 16W1EMS2 register map and employing the Tauri
Sidecar pattern with pkexec, this application delivers a secure, native-feeling
utility that respects the Linux security model while granting users the hardware
control necessary to unlock the full performance potential of their devices.

**Key insight**: The distinction between duty cycle (0-150) and actual RPM
remains a nuance to be managed in the UI, but the underlying control mechanisms
are solid and validated by community data.
