import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';

export interface PrinterInfo {
  name: string;
  is_default: boolean;
  status: string;
}

export interface NetworkDevice {
  ip: string;
  hostname: string | null;
  is_reachable: boolean;
}

export interface BluetoothDevice {
  name: string;
  address: string;
  is_connected: boolean;
}

export interface SystemInfo {
  local_ip: string;
  port: number;
  is_dev: boolean;
  printers: PrinterInfo[];
  autostart_enabled: boolean;
  network_devices: NetworkDevice[];
  bluetooth_devices: BluetoothDevice[];
}

export interface AppSettings {
  port_dev: number;
  port_prod: number;
  active_port: number;
  is_dev: boolean;
  extra: Record<string, string>;
}

@Injectable({ providedIn: 'root' })
export class TauriService {
  async getSystemInfo(): Promise<SystemInfo> {
    return invoke<SystemInfo>('get_system_info');
  }

  async getPrinters(): Promise<PrinterInfo[]> {
    return invoke<PrinterInfo[]>('get_printers');
  }

  async getLocalIp(): Promise<string> {
    return invoke<string>('get_local_ip');
  }

  async getAppPort(): Promise<number> {
    return invoke<number>('get_app_port');
  }

  async printTest(printerName: string, size: 'a4' | 'thermal_50mm' | 'thermal_80mm'): Promise<string> {
    return invoke<string>('print_test', { printerName, size });
  }

  async getAutostartEnabled(): Promise<boolean> {
    return invoke<boolean>('get_autostart_enabled');
  }

  async setAutostartEnabled(enabled: boolean): Promise<void> {
    return invoke<void>('set_autostart_enabled', { enabled });
  }

  async getSettings(): Promise<AppSettings> {
    return invoke<AppSettings>('get_settings');
  }

  async setSetting(key: string, value: string): Promise<void> {
    return invoke<void>('set_setting', { key, value });
  }

  async scanNetwork(): Promise<NetworkDevice[]> {
    return invoke<NetworkDevice[]>('scan_network');
  }

  async getBluetoothDevices(): Promise<BluetoothDevice[]> {
    return invoke<BluetoothDevice[]>('get_bluetooth_devices');
  }

  async renamePrinter(printerName: string, newName: string): Promise<string> {
    return invoke<string>('rename_printer', { printerName, newName });
  }
}
