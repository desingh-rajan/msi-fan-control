export interface FanStatus {
  cpu_temp: number;
  gpu_temp: number;
  fan1_rpm: number;
  fan2_rpm: number;
  cooler_boost: boolean;
  fan_mode: string;
}

export interface HardwareInfo {
  cpu_model: string;
  gpu_model: string;
  memory_total: number;
}

export interface SystemStats {
  memory_used: number;
  memory_total: number;
  swap_used: number;
  swap_total: number;
  cpu_global_frequency: number;
}

export interface CpuCoreDetail {
  name: string;
  frequency: number;
  usage: number;
}
