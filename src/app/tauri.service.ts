import { Injectable, signal } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';

export interface PrinterInfo {
  name: string;
  is_default: boolean;
  status: string;
}

export interface SystemInfo {
  local_ip: string;
  port: number;
  printers: PrinterInfo[];
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
}
