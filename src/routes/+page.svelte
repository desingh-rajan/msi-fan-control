<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";

  import logo from "$lib/assets/logo.png";
  import "./page.css";

  interface FanStatus {
    cpu_temp: number;
    gpu_temp: number;
    fan1_rpm: number;
    fan2_rpm: number;
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
  // pollTimer is defined below with startPolling
  let initialLoading = true;
  let appVersion = "";

  async function connect() {
    try {
      status = await invoke<FanStatus>("start_sidecar");
    } catch (e) {
      error = String(e);
      console.error("Connection failed:", e);
      throw e; // Propagate so caller feels the pain
    }
  }

  let pollTimer: any;
  let isPolling = false;
  let lastPollTime = 0;

  async function startPolling() {
    if (isPolling) return;
    isPolling = true;

    const poll = async () => {
      if (!isPolling) return;

      try {
        status = await invoke<FanStatus>("get_status");
        error = null;
        lastPollTime = Date.now();
      } catch (e) {
        console.warn("Poll error:", e);
        // If error looks like calling sidecar failed, try to reconnect
        // "Sidecar not running" is our explicit error from lib.rs
        if (
          String(e).includes("Sidecar not running") ||
          String(e).includes("Broken pipe") ||
          String(e).includes("timeout")
        ) {
          console.log("Attempting auto-reconnect...");
          try {
            await connect();
            lastPollTime = Date.now();
          } catch (connErr) {
            console.error("Auto-reconnect failed:", connErr);
          }
        }
      }

      // Schedule next poll - recursive approach ensures no overlap
      // and waits for current poll to finish before scheduling next
      if (isPolling) {
        pollTimer = setTimeout(poll, 2000);
      }
    };

    poll();
  }

  function stopPolling() {
    isPolling = false;
    if (pollTimer) clearTimeout(pollTimer);
  }

  async function refresh() {
    loading = true;
    error = null;
    
    // Stop polling first to clear any hung state
    stopPolling();
    
    try {
      // Force a full reconnect on refresh
      console.log("Refresh: forcing reconnect...");
      await connect();
      // If connect worked, start polling
      startPolling();
    } catch (e) {
      console.error("Refresh reconnect failed:", e);
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function toggleCoolerBoost(e: Event) {
    const checkbox = e.target as HTMLInputElement;
    const newState = checkbox.checked;

    try {
      await invoke("set_cooler_boost", { enabled: newState });
      // Immediately refresh status
      status = await invoke<FanStatus>("get_status");
    } catch (err) {
      console.error("Failed to toggle:", err);
      checkbox.checked = !newState;
    } finally {
      localStorage.setItem("cooler_boost", String(newState));
    }
  }

  /* Theme Settings Logic */
  let showSettings = false;
  let theme = "dark";

  function toggleSettings() {
    showSettings = !showSettings;
  }

  function toggleTheme() {
    theme = theme === "dark" ? "light" : "dark";
    if (theme === "light") {
      document.documentElement.setAttribute("data-theme", "light");
    } else {
      document.documentElement.removeAttribute("data-theme");
    }
    localStorage.setItem("theme", theme);
  }

  // Handle visibility change (sleep/wake, window focus)
  function handleVisibilityChange() {
    if (!document.hidden) {
      // App became visible again
      console.log("App visible again, checking poll health");
      
      // Check if polling is stale (no update in last 10 seconds)
      const timeSinceLastPoll = Date.now() - lastPollTime;
      const isStale = timeSinceLastPoll > 10000;
      
      if (isStale || !isPolling) {
        console.log("Polling appears stale or stopped, forcing reconnect");
        stopPolling();
        // Force reconnect after visibility change if polling is stale
        connect()
          .then(() => {
            console.log("Reconnected successfully on visibility change");
            startPolling();
          })
          .catch((e) => {
            console.error("Failed to reconnect on visibility change:", e);
            // Try to start polling anyway - it will attempt reconnect
            startPolling();
          });
      } else {
        // Polling is healthy, just ensure it's running
        startPolling();
      }
    }
  }

  onMount(async () => {
    // Load saved theme
    const savedTheme = localStorage.getItem("theme");
    if (savedTheme) {
      theme = savedTheme;
      if (theme === "light") {
        document.documentElement.setAttribute("data-theme", "light");
      }
    }

    // Load saved cooler boost state and apply it
    const savedCoolerBoost = localStorage.getItem("cooler_boost");
    if (savedCoolerBoost === "true") {
      try {
        await invoke("set_cooler_boost", { enabled: true });
      } catch (e) {
        console.error("Failed to restore cooler boost state:", e);
      }
    }

    try {
      // Parallel fetch
      const [hwInfo, _] = await Promise.all([
        invoke<HardwareInfo>("get_hardware_info").catch((e) => null),
        connect().catch(() => {}), // catch here so Promise.all doesn't fail
      ]);
      if (hwInfo) hardware = hwInfo as HardwareInfo;

      // Start polling loop regardless of initial connect success
      // The poll loop handles reconnection
      startPolling();
    } catch (e) {
      console.error(e);
    }

    loading = false;
    appVersion = await getVersion();

    // Hide initial loading screen after everything is ready
    setTimeout(() => {
      initialLoading = false;
    }, 500);

    // Set up visibility change listener
    document.addEventListener("visibilitychange", handleVisibilityChange);
    window.addEventListener("focus", handleVisibilityChange);
  });

  onDestroy(() => {
    stopPolling();
    document.removeEventListener("visibilitychange", handleVisibilityChange);
    window.removeEventListener("focus", handleVisibilityChange);
  });
</script>

<div
  id="app-container"
  data-theme={theme}
  class="h-screen flex flex-col transition-colors duration-500"
  style:box-shadow={status?.cooler_boost
    ? "inset 0 0 100px rgba(255, 77, 77, 0.05)"
    : "none"}
>
  {#if initialLoading}
    <div
      id="loader-screen"
      class="fixed inset-0 z-50 bg-[#0a0b10]/95 backdrop-blur-xl flex flex-col items-center justify-center transition-all duration-700"
    >
      <img
        src={logo}
        alt="Logo"
        class="loading-logo w-48 h-48 mb-6 object-contain drop-shadow-[0_0_25px_rgba(56,189,248,0.3)]"
      />

      <div class="text-xl font-bold tracking-tight text-white mb-2">
        MSI Fan Control
      </div>

      <div
        class="text-sm text-cyan-400 font-medium tracking-wide animate-pulse mb-8"
      >
        Preparing to chill the beast...
      </div>

      <div class="loader-bar w-64 h-1 bg-white/10 rounded-full overflow-hidden">
        <div
          class="loader-progress bg-gradient-to-r from-transparent via-cyan-400 to-transparent"
        ></div>
      </div>

      <div class="mt-4 text-[10px] text-slate-500 font-mono">
        v{appVersion || "..."}
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
      <button
        class="p-2 hover:bg-white/5 rounded-full transition-colors"
        on:click={refresh}
        title="Refresh Connection"
      >
        <span
          class="material-symbols-outlined text-slate-400 text-xl {loading
            ? 'animate-spin'
            : ''}">refresh</span
        >
      </button>
      <button
        class="p-2 hover:bg-white/5 rounded-full transition-colors"
        on:click={toggleSettings}
      >
        <span class="material-symbols-outlined text-slate-400 text-xl"
          >settings</span
        >
      </button>
    </div>
  </header>

  <!-- Settings Modal -->
  {#if showSettings}
    <div
      class="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm flex items-center justify-center p-4 animate-fade-in"
      on:click|self={toggleSettings}
      on:keydown|self={(e) => e.key === "Escape" && toggleSettings()}
      role="button"
      tabindex="0"
    >
      <div
        class="glass-card w-full max-w-md rounded-2xl p-6 relative overflow-hidden"
      >
        <div class="flex items-center justify-between mb-8">
          <h2 class="text-xl font-bold">Settings</h2>
          <button
            on:click={toggleSettings}
            class="p-1 rounded-full hover:bg-white/5 transition-colors"
          >
            <span class="material-symbols-outlined">close</span>
          </button>
        </div>

        <!-- Theme Toggle -->
        <div
          class="flex items-center justify-between p-4 rounded-xl border border-white/5 bg-white/5"
        >
          <div class="flex items-center gap-3">
            <span
              class="material-symbols-outlined {theme === 'dark'
                ? 'text-blue-400'
                : 'text-orange-400'}"
            >
              {theme === "dark" ? "dark_mode" : "light_mode"}
            </span>
            <div>
              <div class="text-sm font-bold">App Theme</div>
              <div class="text-[10px] text-slate-500 font-semibold uppercase">
                {theme === "dark" ? "Dark Mode" : "Light Mode"}
              </div>
            </div>
          </div>

          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              class="sr-only toggle-checkbox"
              checked={theme === "light"}
              on:change={toggleTheme}
            />
            <div class="toggle-bg w-12 h-7 toggle-track rounded-full"></div>
          </label>
        </div>

        <div class="mt-8 text-center text-[10px] text-slate-500 uppercase">
          MSI Fan Control v{appVersion}
        </div>
      </div>
    </div>
  {/if}

  {#if error}
    <div
      class="bg-red-500/10 border-b border-red-500/20 px-8 py-2 text-xs text-red-400"
    >
      <span class="font-bold">ERROR:</span>
      {error}
    </div>
  {/if}

  <!-- Main Content -->
  <main class="flex-1 overflow-y-auto p-8 max-w-6xl mx-auto w-full">
    <!-- Sensor Grid -->
    <!-- Dual Fan Control Center -->
    <!-- Controls -->
    <div class="glass-card rounded-2xl p-6 flex flex-col justify-between mb-8">
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
          class="p-4 rounded-xl border border-white/5 bg-white/5 flex items-center justify-between"
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
              checked={status?.cooler_boost || false}
              on:change={toggleCoolerBoost}
            />
            <div class="toggle-bg w-12 h-7 bg-slate-700 rounded-full"></div>
          </label>
        </div>

        <!-- Silent (Disabled) -->
        <!--
        <div
          class="p-4 rounded-xl border border-white/5 flex items-center justify-between opacity-60 mt-4"
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
        -->
      </div>
    </div>

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
        <div class="flex flex-col items-end">
          <div class="flex items-baseline gap-2">
            <span
              class="text-4xl font-extrabold tracking-tighter transition-all duration-300"
              class:text-red-500={(status?.cpu_temp ?? 0) > 85}
            >
              {status?.cpu_temp ?? "--"}
            </span>
            <span class="text-xl text-slate-500 font-light">°C</span>
          </div>
          <div class="flex items-center gap-2 mt-2">
            <span
              class="text-xs text-slate-500 font-bold uppercase tracking-wider"
              >Fan 1</span
            >
            <span class="text-lg font-mono font-bold text-slate-300"
              >{status?.fan1_rpm ?? 0}
              <span class="text-xs text-slate-500 font-normal">RPM</span></span
            >
          </div>
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
        <div class="flex flex-col items-end">
          <div class="flex items-baseline gap-2">
            <span
              class="text-4xl font-extrabold tracking-tighter transition-all duration-300"
            >
              {status?.gpu_temp ?? "--"}
            </span>
            <span class="text-xl text-slate-500 font-light">°C</span>
          </div>
          <div class="flex items-center gap-2 mt-2">
            <span
              class="text-xs text-slate-500 font-bold uppercase tracking-wider"
              >Fan 2</span
            >
            <span class="text-lg font-mono font-bold text-slate-300"
              >{status?.fan2_rpm ?? 0}
              <span class="text-xs text-slate-500 font-normal">RPM</span></span
            >
          </div>
        </div>
      </div>
    </div>
  </main>
</div>
