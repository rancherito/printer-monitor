import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { PrinterInfo, SystemInfo, NetworkConfig } from '../models';

@Injectable({ providedIn: 'root' })
export class TauriService {
  getSystemInfo() {
    return invoke<SystemInfo>('get_system_info');
  }

  getPrinters() {
    return invoke<PrinterInfo[]>('get_printers');
  }

  renamePrinter(queueName: string, newName: string) {
    return invoke<string>('rename_printer', { printerName: queueName, newName });
  }

  printTest(queueName: string, size: string) {
    return invoke<string>('print_test', { printerName: queueName, size });
  }

  printTestPdfInternal(queueName: string, size: string) {
    return invoke<string>('print_test_pdf_internal', { printerName: queueName, size });
  }

  printTestA4Pdf(queueName: string, size: string) {
    return invoke<string>('print_test_a4_pdf', { printerName: queueName, size });
  }

  printTestTcp(ip: string, size: string) {
    return invoke<string>('print_test_tcp', { ip, size });
  }

  testUsbPrinter(port: string, size: string) {
    return invoke<string>('test_usb_printer', { port, size });
  }

  addNetworkPrinter(ip: string, name: string) {
    return invoke<string>('add_network_printer', { ip, name });
  }

  addUsbPrinter(port: string, name: string, mode: 'system' | 'app') {
    return invoke<string>('add_usb_printer', { port, name, mode });
  }

  clearPrintQueue(queueName: string) {
    return invoke<string>('clear_print_queue', { printerName: queueName });
  }

  removeCustomPrinter(alias: string) {
    return invoke<string>('remove_custom_printer', { alias });
  }

  scanTcpIpPrinters(subnet: string) {
    return invoke<string[]>('scan_tcp_ip_printers', { subnet });
  }

  getNetworkConfig() {
    return invoke<NetworkConfig>('get_network_config');
  }

  getSerialPorts() {
    return invoke<string[]>('get_serial_ports');
  }

  getAutostartEnabled() {
    return invoke<boolean>('get_autostart_enabled');
  }

  setAutostartEnabled(enabled: boolean) {
    return invoke<void>('set_autostart_enabled', { enabled });
  }
}
