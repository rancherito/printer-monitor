import { Injectable, signal, computed } from '@angular/core';
import { TauriService } from '../services/tauri.service';
import { PrinterInfo } from '../models';

// ─── Guards Angular ───────────────────────────────────────────────────────────
export function guardNonEmpty(value: string): string | null {
  return value.trim().length > 0 ? null : 'El nombre no puede estar vacío.';
}

export function guardValidIp(ip: string): string | null {
  return /^\d{1,3}(\.\d{1,3}){3}$/.test(ip.trim()) ? null : 'IP inválida.';
}

export function guardPortSelected(port: string | null): string | null {
  return port ? null : 'Selecciona un puerto USB.';
}

export function guardIpSelected(ip: string | null): string | null {
  return ip ? null : 'Selecciona una IP de la lista.';
}

// ─── Servicio ─────────────────────────────────────────────────────────────────
@Injectable({ providedIn: 'root' })
export class PrintersService {
  private readonly tauri = new TauriService();

  readonly printers = signal<PrinterInfo[]>([]);
  readonly loading = signal(false);
  readonly error = signal<string | null>(null);

  // TCP dialog state
  readonly tcpDialogOpen = signal(false);
  readonly tcpScanning = signal(false);
  readonly tcpFoundIps = signal<string[]>([]);
  readonly tcpSelectedIp = signal<string | null>(null);
  readonly tcpAlias = signal('');
  readonly tcpResult = signal<string | null>(null);

  // USB dialog state
  readonly usbDialogOpen = signal(false);
  readonly usbPorts = signal<string[]>([]);
  readonly usbSelectedPort = signal<string | null>(null);
  readonly usbAlias = signal('');
  readonly usbResult = signal<string | null>(null);

  readonly osPrinters = computed(() => this.printers().filter(p => p.source === 'os'));
  readonly appPrinters = computed(() => this.printers().filter(p => p.source === 'app'));

  async loadPrinters(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      const list = await this.tauri.getPrinters();
      this.printers.set(list);
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.loading.set(false);
    }
  }

  async printTest(queueName: string, size: string): Promise<void> {
    try {
      await this.tauri.printTest(queueName, size);
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async clearQueue(queueName: string): Promise<void> {
    try {
      await this.tauri.clearPrintQueue(queueName);
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async renamePrinter(queueName: string, newName: string): Promise<void> {
    const err = guardNonEmpty(newName);
    if (err) { this.error.set(err); return; }
    try {
      await this.tauri.renamePrinter(queueName, newName);
      await this.loadPrinters();
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async removeCustomPrinter(alias: string): Promise<void> {
    try {
      await this.tauri.removeCustomPrinter(alias);
      this.printers.update(list => list.filter(p => p.name !== alias));
    } catch (e) {
      this.error.set(String(e));
    }
  }

  // ─── TCP Dialog ───────────────────────────────────────────────────────────
  openTcpDialog(): void {
    this.tcpFoundIps.set([]);
    this.tcpSelectedIp.set(null);
    this.tcpAlias.set('');
    this.tcpResult.set(null);
    this.tcpDialogOpen.set(true);
  }

  closeTcpDialog(): void { this.tcpDialogOpen.set(false); }

  async scanTcpIpPrinters(subnet: string): Promise<void> {
    const err = guardValidIp(subnet.split('.').slice(0, 3).join('.') + '.1');
    if (err) { this.tcpResult.set('Subred inválida.'); return; }
    this.tcpScanning.set(true);
    try {
      const ips = await this.tauri.scanTcpIpPrinters(subnet);
      this.tcpFoundIps.set(ips);
    } catch (e) {
      this.tcpResult.set(String(e));
    } finally {
      this.tcpScanning.set(false);
    }
  }

  async confirmAddTcpPrinter(): Promise<void> {
    const ipErr = guardIpSelected(this.tcpSelectedIp());
    if (ipErr) { this.tcpResult.set(ipErr); return; }
    const nameErr = guardNonEmpty(this.tcpAlias());
    if (nameErr) { this.tcpResult.set(nameErr); return; }
    const ipValidErr = guardValidIp(this.tcpSelectedIp()!);
    if (ipValidErr) { this.tcpResult.set(ipValidErr); return; }

    try {
      await this.tauri.addNetworkPrinter(this.tcpSelectedIp()!, this.tcpAlias());
      await this.loadPrinters();
      this.closeTcpDialog();
    } catch (e) {
      this.tcpResult.set(String(e));
    }
  }

  // ─── USB Dialog ───────────────────────────────────────────────────────────
  async openUsbDialog(): Promise<void> {
    this.usbSelectedPort.set(null);
    this.usbAlias.set('');
    this.usbResult.set(null);
    try {
      const ports = await this.tauri.getSerialPorts();
      this.usbPorts.set(ports);
    } catch (e) {
      this.usbPorts.set([]);
    }
    this.usbDialogOpen.set(true);
  }

  closeUsbDialog(): void { this.usbDialogOpen.set(false); }

  async confirmAddUsbPrinter(): Promise<void> {
    const portErr = guardPortSelected(this.usbSelectedPort());
    if (portErr) { this.usbResult.set(portErr); return; }
    const nameErr = guardNonEmpty(this.usbAlias());
    if (nameErr) { this.usbResult.set(nameErr); return; }

    try {
      await this.tauri.addUsbPrinter(this.usbSelectedPort()!, this.usbAlias());
      await this.loadPrinters();
      this.closeUsbDialog();
    } catch (e) {
      this.usbResult.set(String(e));
    }
  }
}
