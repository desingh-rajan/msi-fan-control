<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  import logo from "$lib/assets/logo.png";

  interface FanStatus {
    cpu_temp: number;
    gpu_temp: number;
    cooler_boost: boolean;
  }

  interface HardwareInfo {
    cpu_model: string;
    gpu_model: string;
  }

  let status: FanStatus | null = null;
  let hardware: HardwareInfo | null = null;
  let loading = true;
  let error: string | null = null;
  let pollInterval: any;
  let initialLoading = true;

  async function connect() {
    try {
      status = await invoke<FanStatus>("start_sidecar");
      startPolling();
    } catch (e) {
      error = String(e);
      console.error("Connection failed:", e);
    }
  }

  function startPolling() {
    pollInterval = setInterval(async () => {
      try {
        status = await invoke<FanStatus>("get_status");
        error = null;
      } catch (e) {
        console.error("Poll error:", e);
      }
    }, 2000);
  }

  async function toggleCoolerBoost(e: Event) {
    const checkbox = e.target as HTMLInputElement;
    const newState = checkbox.checked;

    try {
      await invoke("set_cooler_boost", { enabled: newState });
      // Immediately refresh status
      status = await invoke<FanStatus>("get_status");
    } catch (err) {
      error = String(err);
      console.error("Failed to toggle:", err);
      checkbox.checked = !newState;
    }
  }

  onMount(async () => {
    setTimeout(() => {
      initialLoading = false;
    }, 2000);

    try {
      // Parallel fetch
      const [hwInfo, _] = await Promise.all([
        invoke<HardwareInfo>("get_hardware_info").catch((e) => null),
        connect(),
      ]);
      if (hwInfo) hardware = hwInfo as HardwareInfo;
    } catch (e) {
      console.error(e);
    }

    loading = false;
  });

  onDestroy(() => {
    if (pollInterval) clearInterval(pollInterval);
  });
</script>

<div
  id="app-container"
  class="h-screen flex flex-col transition-opacity duration-1000"
  style:box-shadow={status?.cooler_boost
    ? "inset 0 0 100px rgba(255, 77, 77, 0.05)"
    : "none"}
>
  {#if initialLoading}
    <div
      id="loader-screen"
      class="fixed inset-0 z-50 bg-[#0a0b10] flex flex-col items-center justify-center transition-all duration-700"
    >
      <img
        src={logo}
        alt="Logo"
        class="loading-logo w-32 h-32 mb-8 object-contain"
      />
      <div
        class="text-xs uppercase tracking-[0.3em] text-blue-400 mb-4 font-semibold"
      >
        Initializing System Diagnostics
      </div>
      <div class="loader-bar"><div class="loader-progress"></div></div>
      <div class="mt-8 text-[10px] text-slate-500 uppercase tracking-widest">
        Hardware Interface Ready
      </div>
    </div>
  {/if}

  <!-- Header -->
  <header
    class="h-16 border-b border-white/5 flex items-center justify-between px-8 shrink-0"
  >
    <div class="flex items-center gap-4">
      <div
        class="bg-gradient-to-br from-blue-500/20 to-red-500/20 p-2 rounded-lg"
      >
        <img src={logo} alt="Logo" class="w-6 h-6 object-contain" />
      </div>
      <div>
        <h1 class="font-bold tracking-tight text-lg">MSI Fan Control</h1>
        <p
          class="text-[10px] text-slate-500 uppercase tracking-widest font-semibold"
        >
          Model GF65 Thin 10SDR
        </p>
      </div>
    </div>
    <div class="flex items-center gap-6">
      <div class="flex items-center gap-2">
        <div
          class="status-dot"
          style:background={status ? "#10b981" : "#ef4444"}
          style:box-shadow={status ? "0 0 12px #10b981" : "0 0 12px #ef4444"}
        ></div>
        <span
          class="text-xs text-slate-400 font-medium uppercase tracking-wider"
          >{status ? "System Optimal" : "Connecting..."}</span
        >
      </div>
      <button class="p-2 hover:bg-white/5 rounded-full transition-colors">
        <span class="material-symbols-outlined text-slate-400 text-xl"
          >settings</span
        >
      </button>
    </div>
  </header>

  <!-- Main Content -->
  <main class="flex-1 overflow-y-auto p-8 max-w-6xl mx-auto w-full">
    <!-- Sensor Grid -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6 mb-8">
      <!-- CPU Card -->
      <div class="glass-card rounded-2xl p-6 relative overflow-hidden group">
        <div
          class="absolute top-0 left-0 w-1 h-full bg-red-500 opacity-50"
        ></div>
        <div class="flex justify-between items-start mb-6">
          <div class="flex flex-col gap-1">
            <span
              class="text-xs text-slate-500 font-bold uppercase tracking-wider"
              >Processor</span
            >
            <h3
              class="text-xl font-bold truncate pr-4"
              title={hardware?.cpu_model}
            >
              {hardware?.cpu_model ?? "Intializing..."}
            </h3>
          </div>
          <span
            class="material-symbols-outlined text-red-500/50 group-hover:text-red-500 transition-colors"
            >thermostat</span
          >
        </div>
        <div class="flex items-baseline gap-2">
          <span
            class="text-5xl font-extrabold tracking-tighter transition-all duration-300"
            class:text-red-500={(status?.cpu_temp ?? 0) > 85}
          >
            {status?.cpu_temp ?? "--"}
          </span>
          <span class="text-2xl text-slate-500 font-light">°C</span>
        </div>
      </div>

      <!-- GPU Card -->
      <div class="glass-card rounded-2xl p-6 relative overflow-hidden group">
        <div
          class="absolute top-0 left-0 w-1 h-full bg-blue-500 opacity-50"
        ></div>
        <div class="flex justify-between items-start mb-6">
          <div class="flex flex-col gap-1">
            <span
              class="text-xs text-slate-500 font-bold uppercase tracking-wider"
              >Graphics</span
            >
            <h3
              class="text-xl font-bold truncate pr-4"
              title={hardware?.gpu_model}
            >
              {hardware?.gpu_model ?? "Initializing..."}
            </h3>
          </div>
          <span
            class="material-symbols-outlined text-blue-500/50 group-hover:text-blue-500 transition-colors"
            >videogame_asset</span
          >
        </div>
        <div class="flex items-baseline gap-2">
          <span
            class="text-5xl font-extrabold tracking-tighter transition-all duration-300"
          >
            {status?.gpu_temp ?? "--"}
          </span>
          <span class="text-2xl text-slate-500 font-light">°C</span>
        </div>
      </div>
    </div>

    <!-- Dual Fan Control Center -->
    <!-- Controls -->
    <div class="glass-card rounded-2xl p-6 flex flex-col justify-between">
      <div>
        <div class="flex items-center gap-3 mb-8">
          <span class="material-symbols-outlined text-slate-400">tune</span>
          <span
            class="text-xs text-slate-400 font-bold uppercase tracking-wider"
            >Fan Controls</span
          >
        </div>

        <!-- Cooler Boost -->
        <div
          class="p-4 rounded-xl border border-white/5 bg-white/5 flex items-center justify-between mb-4"
        >
          <div class="flex items-center gap-3">
            <span class="material-symbols-outlined text-red-500">bolt</span>
            <div>
              <div class="text-sm font-bold">Cooler Boost</div>
              <div class="text-[10px] text-slate-500 font-semibold uppercase">
                Maximum Speed
              </div>
            </div>
          </div>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              class="sr-only toggle-checkbox"
              checked={status?.cooler_boost}
              on:change={toggleCoolerBoost}
            />
            <div class="toggle-bg w-12 h-7 bg-slate-700 rounded-full"></div>
          </label>
        </div>

        <!-- Silent (Disabled) -->
        <div
          class="p-4 rounded-xl border border-white/5 flex items-center justify-between opacity-60"
        >
          <div class="flex items-center gap-3">
            <span class="material-symbols-outlined text-blue-400">eco</span>
            <div>
              <div class="text-sm font-bold">Silent Profile</div>
              <div class="text-[10px] text-slate-500 font-semibold uppercase">
                Low Acoustic
              </div>
            </div>
          </div>
          <label class="relative inline-flex items-center cursor-not-allowed">
            <input type="checkbox" disabled class="sr-only" />
            <div class="toggle-bg w-12 h-7 bg-slate-800 rounded-full"></div>
          </label>
        </div>
      </div>
    </div>
  </main>

  <footer
    class="h-10 border-t border-white/5 px-8 flex items-center justify-between shrink-0 bg-black/20"
  >
    <div class="flex gap-4">
      <span class="text-[10px] text-slate-500 uppercase font-semibold"
        >Ready</span
      >
    </div>
    <div class="flex items-center gap-2">
      <span class="material-symbols-outlined text-xs text-green-500"
        >verified</span
      >
      <span
        class="text-[10px] text-slate-500 uppercase font-semibold tracking-tighter"
        >Hardware Authentication Secure</span
      >
    </div>
  </footer>
</div>

<style>
  :global(:root) {
    --bg-deep: #0a0b10;
    --surface: #16181d;
    --surface-accent: #1f2229;
    --accent-primary: #ff4d4d;
    --accent-secondary: #4d94ff;
  }

  /* Glassmorphism */
  .glass-card {
    background: rgba(22, 24, 29, 0.7);
    backdrop-filter: blur(12px);
    border: 1px solid rgba(255, 255, 255, 0.05);
    transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  }
  .glass-card:hover {
    border-color: rgba(255, 255, 255, 0.1);
    transform: translateY(-2px);
    box-shadow: 0 10px 30px -10px rgba(0, 0, 0, 0.5);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #10b981;
    box-shadow: 0 0 12px #10b981;
  }

  /* Scrollbars */
  ::-webkit-scrollbar {
    width: 6px;
  }
  ::-webkit-scrollbar-track {
    background: var(--bg-deep);
  }
  ::-webkit-scrollbar-thumb {
    background: var(--surface-accent);
    border-radius: 10px;
  }

  /* Toggle Switch */
  .toggle-bg:after {
    content: "";
    position: absolute;
    top: 4px;
    left: 4px;
    background: white;
    border-radius: 99px;
    height: 20px;
    width: 20px;
    transition: 0.3s;
  }
  .toggle-checkbox:checked + .toggle-bg:after {
    transform: translateX(24px);
  }
  .toggle-checkbox:checked + .toggle-bg {
    background-color: var(--accent-primary);
  }

  /* Loader */
  .loader-bar {
    width: 200px;
    height: 2px;
    background: var(--surface-accent);
    position: relative;
    overflow: hidden;
    border-radius: 4px;
  }
  .loader-progress {
    position: absolute;
    left: -100%;
    width: 100%;
    height: 100%;
    background: linear-gradient(
      90deg,
      transparent,
      var(--accent-secondary),
      transparent
    );
    animation: progress-move 1.5s infinite linear;
  }
  @keyframes progress-move {
    0% {
      left: -100%;
    }
    100% {
      left: 100%;
    }
  }
  .loading-logo {
    width: 80px;
    height: 80px;
    margin-bottom: 2rem;
    animation: pulse-ring 2s infinite ease-in-out;
  }
  @keyframes pulse-ring {
    0% {
      transform: scale(0.95);
      opacity: 0.5;
    }
    50% {
      transform: scale(1.05);
      opacity: 1;
    }
    100% {
      transform: scale(0.95);
      opacity: 0.5;
    }
  }
</style>
