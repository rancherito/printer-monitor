import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';

export interface PrinterInfo {
  /** Nombre visible para el usuario (display name del SO). */
  name: string;
  /** Nombre interno de la cola CUPS / Windows printer name. Usar para operaciones de backend (print, rename). */
  queue_name: string;
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

export interface SerialPort {
  port_name: string;
  description: string;
  device_type: 'USB-Serial' | 'USB-CDC' | 'COM' | string;
}

export interface SystemInfo {
  local_ip: string;
  port: number;
  is_dev: boolean;
  printers: PrinterInfo[];
  serial_ports: SerialPort[];
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

export interface NetworkConfig {
  ip: string;
  mask: string;
  gateway: string;
  interface: string;
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

  async printTest(printerName: string, size: 'a4' | 'thermal_58mm' | 'thermal_80mm'): Promise<string> {
    return invoke<string>('print_test', { printerName, size });
  }

  /**
   * Envía una página de prueba ESC/POS directamente al puerto USB/serie indicado,
   * sin pasar por CUPS. Usar para impresoras térmicas no registradas en el sistema.
   */
  async printTestUsb(portName: string, size: 'thermal_58mm' | 'thermal_80mm'): Promise<string> {
    return invoke<string>('print_test_usb', { portName, size });
  }

  /**
   * Envía una página de prueba ESC/POS directamente a una IP:9100
   * sin registrar la impresora en CUPS.
   */
  async printTestTcp(ip: string, size: 'thermal_58mm' | 'thermal_80mm'): Promise<string> {
    return invoke<string>('print_test_tcp', { ip, size });
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

  async getSerialPorts(): Promise<SerialPort[]> {
    return invoke<SerialPort[]>('get_serial_ports');
  }

  async scanTcpIpPrinters(ip: string, mask: string): Promise<string[]> {
    return invoke<string[]>('scan_tcp_ip_printers', { ip, mask });
  }

  async getNetworkConfig(): Promise<NetworkConfig> {
    return invoke<NetworkConfig>('get_network_config');
  }

  async setNetworkConfig(ip: string, mask: string, gateway: string): Promise<string> {
    return invoke<string>('set_network_config', { ip, mask, gateway });
  }

  async restoreNetworkDhcp(): Promise<string> {
    return invoke<string>('restore_network_dhcp');
  }

  /**
   * Imprime un PDF (base64) en la impresora indicada.
   * Rust lo convierte a imagen con sips y lo envía vía ESC*.
   */
  async printPdf(pdfB64: string, printerName: string, width: '58mm' | '80mm'): Promise<string> {
    return invoke<string>('print_pdf', { pdfB64, printerName, width });
  }

  async addNetworkPrinter(ip: string, name: string): Promise<string> {
    return invoke<string>('add_network_printer', { ip, name });
  }

  /**
   * Registra una impresora USB en CUPS usando su URI detectada por lpinfo.
   */
  async addUsbPrinter(deviceName: string, cupsName: string): Promise<string> {
    return invoke<string>('add_usb_printer', { deviceName, cupsName });
  }

  async clearPrintQueue(printerName: string): Promise<string> {
    return invoke<string>('clear_print_queue', { printerName });
  }

  /**
   * Imprime un ticket con título, texto e imagen rasterizada en impresora térmica.
   * La imagen se convierte a bitmap 1bpp (Floyd-Steinberg) y se envía por ESC/POS GS v 0.
   * @param imageB64 Imagen PNG o JPEG codificada en base64 (vacío = sin imagen)
   */
  async printImageTicket(
    printerName: string,
    title: string,
    bodyLines: string[],
    imageB64: string,
    size: 'thermal_58mm' | 'thermal_80mm',
  ): Promise<string> {
    return invoke<string>('print_image_ticket', { printerName, title, bodyLines, imageB64, size });
  }
}
