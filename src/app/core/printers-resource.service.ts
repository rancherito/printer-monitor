import { Injectable, computed, effect, inject, resource, signal } from '@angular/core';
import { TauriService } from '../services/tauri.service';
import { LogService } from '../services/log.service';
import { SystemResourceService } from './system-resource.service';
import { PrinterInfo, UsbDevice } from '../models';

export function guardNonEmpty(value: string): string | null {
  return value.trim().length > 0 ? null : 'El nombre no puede estar vacío.';
}

export function guardValidIp(ip: string): string | null {
  return /^\d{1,3}(\.\d{1,3}){3}$/.test(ip.trim()) ? null : 'IP inválida.';
}

export function guardPortSelected(port: string | null): string | null {
  return port ? null : 'Selecciona un puerto USB.';
}

export type TestStatus = 'idle' | 'testing' | 'ok' | 'fail';

@Injectable({ providedIn: 'root' })
export class PrintersResourceService {
  private readonly tauri = inject(TauriService);
  private readonly log = inject(LogService);
  private readonly system = inject(SystemResourceService);

  private readonly refresh$ = signal(0);

  readonly printers = resource<PrinterInfo[], number>({
    params: () => this.refresh$(),
    loader: () => this.tauri.getPrinters(),
  });

  readonly osPrinters = computed(() => (this.printers.value() ?? []).filter((p) => p.source === 'os'));
  readonly appPrinters = computed(() => (this.printers.value() ?? []).filter((p) => p.source === 'app'));

  refresh(): void {
    this.refresh$.update((n) => n + 1);
  }

  // ─── USB dialog (recurso reactivo con TTL/invalidación explícita) ─────────
  readonly usbDialogOpen = signal(false);
  readonly usbSelectedPort = signal<string | null>(null);
  readonly usbMode = signal<'system' | 'app'>('system');
  readonly usbAlias = signal('');
  readonly usbResult = signal<string | null>(null);
  readonly usbSize = signal<'58mm' | '80mm'>('58mm');

  readonly usbDevices = resource<UsbDevice[], number>({
    params: () => this.refresh$(),
    loader: () => this.tauri.getUsbDevices(),
  });

  readonly usbPorts = computed(() => this.usbDevices.value() ?? []);

  readonly usbTestStatus = signal<TestStatus>('idle');
  readonly usbTestMsg = signal<string | null>(null);

  // ─── TCP dialog (recurso con subnet como dependencia) ─────────────────────
  readonly tcpDialogOpen = signal(false);
  readonly tcpScanning = signal(false);
  readonly tcpFoundIps = signal<string[]>([]);
  readonly tcpIpInput = signal('');
  readonly tcpAlias = signal('');
  readonly tcpResult = signal<string | null>(null);
  readonly tcpSize = signal<'58mm' | '80mm'>('58mm');

  readonly tcpTestStatus = signal<TestStatus>('idle');
  readonly tcpTestMsg = signal<string | null>(null);

  readonly appError = signal<string | null>(null);
  readonly error = signal<string | null>(null);

  // ─── TCP dialog flow ──────────────────────────────────────────────────────
  openTcpDialog(): void {
    this.tcpFoundIps.set([]);
    this.tcpIpInput.set('');
    this.tcpAlias.set('');
    this.tcpResult.set(null);
    this.tcpTestStatus.set('idle');
    this.tcpTestMsg.set(null);
    this.tcpSize.set('58mm');
    this.tcpDialogOpen.set(true);
  }

  closeTcpDialog(): void {
    this.tcpDialogOpen.set(false);
  }

  async scanTcpIpPrinters(subnet: string): Promise<void> {
    const err = guardValidIp(subnet.split('.').slice(0, 3).join('.') + '.1');
    if (err) {
      this.tcpResult.set('Subred inválida.');
      return;
    }
    this.tcpScanning.set(true);
    this.tcpResult.set(null);
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
    const ip = this.tcpIpInput().trim();
    const ipValidErr = guardValidIp(ip);
    if (ipValidErr) {
      this.tcpResult.set('Ingresa una IP válida (ej. 192.168.1.100).');
      return;
    }
    const nameErr = guardNonEmpty(this.tcpAlias());
    if (nameErr) {
      this.tcpResult.set(nameErr);
      return;
    }

    try {
      await this.tauri.addNetworkPrinter(ip, this.tcpAlias());
      try {
        await this.tauri.printTestTcp(ip, this.tcpSize());
      } catch (e) {
        this.appError.set(`Prueba ${this.tcpAlias()}: ${String(e)}`);
      }
      this.log.log('success', `Impresora TCP/IP agregada: ${this.tcpAlias()} (${ip})`);
      this.refresh();
      this.system.refreshAll();
      this.closeTcpDialog();
    } catch (e) {
      this.tcpResult.set(String(e));
      this.log.log('error', `Error al agregar impresora TCP/IP (${ip})`, String(e));
    }
  }

  async testPrintTcp(): Promise<void> {
    const ip = this.tcpIpInput().trim();
    if (guardValidIp(ip)) {
      this.tcpTestStatus.set('fail');
      this.tcpTestMsg.set('Ingresa una IP válida primero.');
      return;
    }
    this.tcpTestStatus.set('testing');
    this.tcpTestMsg.set(null);
    try {
      await this.tauri.printTestTcp(ip, this.tcpSize());
      this.tcpTestStatus.set('ok');
      this.tcpTestMsg.set('Impresión enviada correctamente.');
    } catch (e) {
      this.tcpTestStatus.set('fail');
      this.tcpTestMsg.set(String(e));
    }
  }

  // ─── USB dialog flow ──────────────────────────────────────────────────────
  openUsbDialog(): void {
    this.usbSelectedPort.set(null);
    this.usbMode.set('system');
    this.usbAlias.set('');
    this.usbResult.set(null);
    this.usbTestStatus.set('idle');
    this.usbTestMsg.set(null);
    this.usbSize.set('58mm');
    this.usbDialogOpen.set(true);
  }

  closeUsbDialog(): void {
    this.usbDialogOpen.set(false);
  }

  async refreshUsbPorts(): Promise<void> {
    this.refresh();
  }

  async testPrintUsb(): Promise<void> {
    const port = this.usbSelectedPort();
    if (!port) {
      this.usbTestStatus.set('fail');
      this.usbTestMsg.set('Selecciona un puerto primero.');
      return;
    }
    this.usbTestStatus.set('testing');
    this.usbTestMsg.set(null);
    try {
      await this.tauri.testUsbPrinter(port, this.usbSize());
      this.usbTestStatus.set('ok');
      this.usbTestMsg.set('Impresión enviada correctamente.');
    } catch (e) {
      this.usbTestStatus.set('fail');
      this.usbTestMsg.set(String(e));
    }
  }

  async confirmAddUsbPrinter(): Promise<void> {
    const portErr = guardPortSelected(this.usbSelectedPort());
    if (portErr) {
      this.usbResult.set(portErr);
      return;
    }
    const nameErr = guardNonEmpty(this.usbAlias());
    if (nameErr) {
      this.usbResult.set(nameErr);
      return;
    }

    const port = this.usbSelectedPort()!;
    const alias = this.usbAlias();
    const mode = this.usbMode();
    const size = this.usbSize();

    try {
      await this.tauri.addUsbPrinter(port, alias, mode);
      try {
        if (mode === 'system') {
          await this.tauri.printTestPdfInternal(alias, size);
        } else {
          await this.tauri.testUsbPrinter(port, size);
        }
      } catch (e) {
        this.appError.set(`Prueba ${alias}: ${String(e)}`);
      }
      this.refresh();
      this.system.refreshAll();
      this.closeUsbDialog();
    } catch (e) {
      this.usbResult.set(String(e));
    }
  }

  // ─── Acciones sobre impresoras ────────────────────────────────────────────
  async printTest(queueName: string, size: string): Promise<void> {
    try {
      await this.tauri.printTest(queueName, size);
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async printTestPdfInternal(queueName: string, size: string): Promise<void> {
    try {
      await this.tauri.printTestPdfInternal(queueName, size);
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async printTestA4Pdf(queueName: string, size: string): Promise<void> {
    try {
      await this.tauri.printTestA4Pdf(queueName, size);
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async printAppTestPdf(queueName: string, size: string): Promise<void> {
    this.appError.set(null);
    try {
      await this.tauri.printTestA4Pdf(queueName, size);
    } catch (e) {
      this.appError.set(String(e));
    }
  }

  async clearQueue(queueName: string): Promise<void> {
    try {
      await this.tauri.clearPrintQueue(queueName);
      this.refresh();
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async renamePrinter(queueName: string, newName: string): Promise<void> {
    const err = guardNonEmpty(newName);
    if (err) {
      this.error.set(err);
      return;
    }
    try {
      await this.tauri.renamePrinter(queueName, newName);
      this.log.log('success', `Impresora "${queueName}" renombrada a "${newName}"`);
      this.refresh();
    } catch (e) {
      this.error.set(String(e));
      this.log.log('error', `Error al renombrar "${queueName}"`, String(e));
    }
  }

  async removeCustomPrinter(alias: string): Promise<void> {
    try {
      await this.tauri.removeCustomPrinter(alias);
      this.log.log('warn', `Impresora personalizada "${alias}" eliminada`);
      this.refresh();
    } catch (e) {
      this.error.set(String(e));
      this.log.log('error', `Error al eliminar "${alias}"`, String(e));
    }
  }

  constructor() {
    // Log reactivo del estado de carga de impresoras
    effect(() => {
      const list = this.printers.value();
      if (list) {
        this.log.log(
          'info',
          `${list.length} impresoras cargadas (${list.filter((p) => p.source === 'os').length} SO, ${list.filter((p) => p.source === 'app').length} app)`,
        );
      }
    });
  }
}
