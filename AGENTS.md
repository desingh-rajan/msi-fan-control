# Agent Guidelines for MSI Fan Control

This document provides coding standards and development guidelines for AI agents working on the MSI Fan Control project.

## Project Overview

MSI Fan Control is a Tauri-based desktop application for Linux that enables fan control for MSI laptops. The project uses:
- **Frontend**: SvelteKit 2 + Svelte 5 + TypeScript + Tailwind CSS
- **Backend**: Rust (Tauri 2) with a privileged sidecar binary
- **Architecture**: Separation between user-space UI and root-privileged hardware access

## Build & Development Commands

### Frontend (SvelteKit)
```bash
npm install                    # Install dependencies
npm run dev                    # Start dev server
npm run build                  # Build for production
npm run preview                # Preview production build
npm run check                  # Type-check TypeScript/Svelte
npm run check:watch            # Type-check in watch mode
```

### Backend (Tauri)
```bash
npm run tauri dev              # Run app in development mode
npm run tauri build            # Build production bundles (.deb, AppImage)

# Build sidecar binary separately
cd src-tauri/binaries/msi-sidecar
cargo build --release          # Compile privileged sidecar
cargo build                    # Debug build
cargo check                    # Check for errors without building
cargo clippy                   # Run linter
cd ../../..

# After building sidecar, copy to expected location
cp src-tauri/binaries/msi-sidecar/target/release/msi-sidecar \
   src-tauri/binaries/msi-sidecar-x86_64-unknown-linux-gnu
```

### Testing
```bash
# No test framework currently configured
# Manual testing through dev mode required
npm run tauri dev
```

### Permissions Setup (Development)
```bash
./scripts/setup-permissions.sh  # Set up pkexec for dev sidecar binary
```

## Code Style Guidelines

### TypeScript/Svelte

#### Imports
- Use `$lib` alias for local imports: `import logo from "$lib/assets/logo.png"`
- Group imports: external libraries first, then local modules
- Tauri imports: `import { invoke } from "@tauri-apps/api/core"`

#### Formatting
- Use 2-space indentation
- Prefer `let` over `const` for mutable state in Svelte components
- Use semicolons consistently
- Double quotes for strings

#### Types
- Enable strict TypeScript: `"strict": true` in tsconfig.json
- Define explicit interfaces for data structures:
```typescript
interface FanStatus {
  cpu_temp: number;
  gpu_temp: number;
  fan1_rpm: number;
  fan2_rpm: number;
  cooler_boost: boolean;
}
```
- Use type annotations for function parameters and return types
- Prefer explicit types over `any`

#### Naming Conventions
- **Variables/Functions**: `camelCase` (e.g., `getCpuTemp`, `fanStatus`)
- **Interfaces/Types**: `PascalCase` (e.g., `FanStatus`, `HardwareInfo`)
- **Constants**: `UPPER_SNAKE_CASE` for true constants
- **Files**: `kebab-case.ts` or `+page.svelte` (SvelteKit convention)

#### Svelte Specifics
- Use Svelte 5 runes: `$state`, `$derived`, `$effect` for reactivity
- Lifecycle: `onMount`, `onDestroy` from `svelte`
- Store state in `let` variables at component top level
- Use `async/await` for Tauri invoke calls
```typescript
const status = await invoke<FanStatus>("get_status");
```

#### Error Handling
- Use try-catch for async operations
- Log errors to console: `console.error("Failed:", e)`
- Display user-friendly error messages in UI
- Convert errors to strings: `String(e)` for display

### Rust

#### Formatting
- Use `rustfmt` defaults (4-space indentation)
- Run `cargo fmt` before committing

#### Naming Conventions
- **Variables/Functions**: `snake_case` (e.g., `get_status`, `fan_rpm`)
- **Types/Structs/Enums**: `PascalCase` (e.g., `FanStatus`, `SidecarResponse`)
- **Constants**: `UPPER_SNAKE_CASE` (e.g., `REG_CPU_TEMP`, `EC_IO_PATH`)
- **Files**: `snake_case.rs`

#### Types & Structs
- Use `#[derive(Debug, Serialize, Deserialize)]` for data types
- Prefer explicit types over type inference for public APIs
- Use `Result<T, String>` for error handling in commands
```rust
#[tauri::command]
async fn get_status(state: State<'_, SidecarState>) -> Result<FanStatus, String>
```

#### Error Handling
- Use `Result<T, E>` for fallible operations
- Convert errors with `.map_err(|e| e.to_string())?`
- Return descriptive error messages
- Use `ok_or()` for Option to Result conversion

#### Tauri Commands
- Annotate with `#[tauri::command]`
- Use `async` for I/O operations
- Accept `State<'_, T>` for shared state
- Return `Result<T, String>` for error handling

#### Sidecar Binary
- Minimal dependencies (serde, serde_json only)
- Use stdin/stdout for JSON-based IPC
- Implement command pattern with serde-tagged enums
- Handle EC I/O with proper error checking

### Tailwind CSS

#### Class Organization
- Use class ordering: layout → sizing → spacing → colors → effects
- Example: `class="flex items-center gap-4 p-6 rounded-xl bg-white/5"`
- Use Tailwind arbitrary values sparingly: `text-[10px]`

#### Theme
- Dark mode default with `data-theme="light"` for light mode
- Use semantic opacity: `bg-white/5`, `border-white/10`
- Glassmorphism: `backdrop-blur-xl`, `bg-gradient-to-br`

## Project Structure

```
msi-fan-control/
├── src/                         # SvelteKit frontend
│   ├── lib/                     # Shared components, assets
│   └── routes/                  # Pages and layouts
│       ├── +page.svelte         # Main UI
│       ├── +layout.svelte       # Root layout
│       └── +layout.ts           # SSR config (disabled)
├── src-tauri/                   # Rust backend
│   ├── src/
│   │   ├── main.rs              # Entry point
│   │   └── lib.rs               # Tauri commands, state management
│   ├── binaries/
│   │   └── msi-sidecar/         # Privileged EC access binary
│   │       └── src/main.rs
│   ├── icons/                   # App icons
│   └── Cargo.toml
├── scripts/
│   └── setup-permissions.sh     # Dev permissions helper
└── package.json
```

## Architecture Patterns

### Frontend-Backend Communication
- Use `invoke()` from `@tauri-apps/api/core` for Rust commands
- Poll status every 2 seconds with `setInterval`
- Handle connection failures gracefully with error states

### State Management
- Svelte component-local state with `let` variables
- Use `localStorage` for persisting settings (theme, cooler_boost)
- Rust side: `Mutex<Option<Child>>` for sidecar process state

### Hardware Access (Sidecar)
- Main Tauri app runs as user, spawns privileged sidecar via `pkexec`
- Sidecar reads/writes EC registers at `/sys/kernel/debug/ec/ec0/io`
- JSON-based stdin/stdout IPC between main app and sidecar

## Important Notes

1. **No Tests**: This project currently has no automated tests. Manual testing required.

2. **Linux-Only**: Requires `ec_sys` kernel module with write support enabled:
   ```bash
   sudo modprobe ec_sys write_support=1
   ```

3. **Security**: Sidecar binary runs with root privileges via pkexec/Polkit. Handle carefully.

4. **Hardware-Specific**: EC register offsets are for MSI GF65 Thin 10SDR. Other models may differ.

5. **SvelteKit SSR**: Disabled via `export const ssr = false` in `+layout.ts` (Tauri requirement).

6. **Single Instance**: App uses `tauri-plugin-single-instance` to focus existing window.

7. **System Tray**: App minimizes to tray instead of closing window.

## Commit Guidelines

- Use conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `chore:`
- Examples:
  - `feat: add temperature warning threshold`
  - `fix: correct fan RPM calculation for 0xCD register`
  - `refactor: extract sidecar IPC into separate module`

## Resources

- [Tauri Docs](https://v2.tauri.app/)
- [SvelteKit Docs](https://kit.svelte.dev/)
- [Svelte 5 Docs](https://svelte.dev/docs/svelte/overview)
- [MControlCenter](https://github.com/dmitry-s93/MControlCenter) - EC register reference
