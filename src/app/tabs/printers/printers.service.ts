import { Injectable, inject, signal, computed } from '@angular/core';
import { TauriService, PrinterInfo, SerialPort } from '../../services/tauri.service';
import { PdfService, PdfPrintWidth } from '../../services/pdf.service';
import { NetworkService } from '../network/network.service';

export type PrintSize = 'thermal_58mm' | 'thermal_80mm';
export type ThermalSize = 'thermal_58mm' | 'thermal_80mm';

@Injectable({ providedIn: 'root' })
export class PrintersService {
  private readonly tauri = inject(TauriService);
  private readonly network = inject(NetworkService);
  private readonly pdf = inject(PdfService);

  // ── Estado de la lista ─────────────────────────────────────────────────
  readonly loading = signal(true);
  readonly printers = signal<PrinterInfo[]>([]);
  readonly serialPorts = signal<SerialPort[]>([]);

  // ── Impresión de prueba ────────────────────────────────────────────────
  readonly printingFor = signal<string | null>(null);
  readonly printResult = signal<{ ok: boolean; message: string } | null>(null);

  // ── Renombrar ──────────────────────────────────────────────────────────
  readonly renamingPrinter = signal<string | null>(null);
  readonly renameValue = signal('');
  readonly renamingFor = signal<string | null>(null);
  readonly renameResult = signal<{ ok: boolean; message: string; printerName: string } | null>(null);

  // ── Limpiar cola ───────────────────────────────────────────────────────
  readonly clearingFor = signal<string | null>(null);

  // ── Escáner TCP/IP ─────────────────────────────────────────────────────
  readonly scanningPrinters = signal(false);
  readonly foundPrinters = signal<string[]>([]);
  // ── Impresión TCP/IP directa (sin registrar) ───────────────────────────
  readonly tcpPrintingFor = signal<string | null>(null);
  readonly tcpPrintResult = signal<{ ok: boolean; message: string } | null>(null);
  // ── Agregar impresora ──────────────────────────────────────────────────
  readonly addingPrinter = signal<string | null>(null);
  readonly printerNameInput = signal('');
  readonly savingPrinter = signal(false);
  // ── Impresión PDF ─────────────────────────────────────────
  readonly pdfPrintingFor = signal<string | null>(null);
  readonly pdfPrintResult = signal<{ ok: boolean; message: string } | null>(null);
  /** URL de objeto (blob:) del último PDF generado — para previsualización. */
  readonly pdfPreviewUrl = signal<string | null>(null);

  async printTestPdf(printer: PrinterInfo, width: PdfPrintWidth): Promise<void> {
    const key = `${printer.queue_name}::pdf_${width}`;
    this.pdfPrintingFor.set(key);
    this.pdfPrintResult.set(null);
    // Revoca la URL anterior para liberar memoria
    const prev = this.pdfPreviewUrl();
    if (prev) URL.revokeObjectURL(prev);
    this.pdfPreviewUrl.set(null);
    try {
      // 1. Generar PDF en base64 (pdfmake — solo frontend)
      const docDef = this.pdf.testDocDef(printer.queue_name);
      const b64 = await this.pdf.toPdfBase64(docDef);

      // 2. Crear URL de objeto para previsualización inmediata
      const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
      const blob = new Blob([bytes], { type: 'application/pdf' });
      this.pdfPreviewUrl.set(URL.createObjectURL(blob));

      // 3. Enviar base64 a Rust para imprimir
      const result = await this.tauri.printPdf(b64, printer.queue_name, width);
      this.pdfPrintResult.set({ ok: true, message: result });
    } catch (e) {
      this.pdfPrintResult.set({ ok: false, message: String(e) });
    } finally {
      this.pdfPrintingFor.set(null);
    }
  }

  closePdfPreview(): void {
    const url = this.pdfPreviewUrl();
    if (url) URL.revokeObjectURL(url);
    this.pdfPreviewUrl.set(null);
  }

  isPdfPrinting(printer: PrinterInfo, width: PdfPrintWidth): boolean {
    return this.pdfPrintingFor() === `${printer.queue_name}::pdf_${width}`;
  }
  // ── Impresoras USB detectadas (sin registrar en CUPS) ──────────────────
  readonly usbPrinters = computed(() =>
    this.serialPorts().filter(p => p.device_type === 'USB-Printer')
  );
  readonly usbAddingFor = signal<string | null>(null);  // port_name del dispositivo
  readonly usbPrinterNameInput = signal('');
  readonly usbSavingPrinter = signal(false);
  readonly usbAddResult = signal<{ ok: boolean; message: string } | null>(null);

  async refresh(): Promise<void> {
    this.loading.set(true);
    try {
      const printers = await this.tauri.getPrinters();
      this.printers.set(printers);
    } catch (e) {
      console.error('Error al obtener impresoras:', e);
    } finally {
      this.loading.set(false);
    }
  }

  updateFromSystemInfo(printers: PrinterInfo[], serialPorts: SerialPort[]): void {
    this.printers.set(printers);
    this.serialPorts.set(serialPorts);
    this.loading.set(false);
  }

  async printTest(printer: PrinterInfo, size: PrintSize): Promise<void> {
    const key = `${printer.queue_name}::${size}`;
    this.printingFor.set(key);
    this.printResult.set(null);
    try {
      const result = await this.tauri.printTest(printer.queue_name, size);
      this.printResult.set({ ok: true, message: result });
    } catch (e) {
      this.printResult.set({ ok: false, message: String(e) });
    } finally {
      this.printingFor.set(null);
    }
  }

  isPrinting(printer: PrinterInfo, size: PrintSize): boolean {
    return this.printingFor() === `${printer.queue_name}::${size}`;
  }

  async clearQueue(printer: PrinterInfo): Promise<void> {
    this.clearingFor.set(printer.queue_name);
    this.printResult.set(null);
    try {
      const result = await this.tauri.clearPrintQueue(printer.queue_name);
      this.printResult.set({ ok: true, message: result });
    } catch (e) {
      this.printResult.set({ ok: false, message: String(e) });
    } finally {
      this.clearingFor.set(null);
    }
  }

  startRename(printer: PrinterInfo): void {
    this.renamingPrinter.set(printer.queue_name);
    this.renameValue.set(printer.name);
    this.renameResult.set(null);
  }

  cancelRename(): void {
    this.renamingPrinter.set(null);
    this.renameValue.set('');
  }

  async confirmRename(printer: PrinterInfo): Promise<void> {
    const newName = this.renameValue().trim();
    if (!newName || newName === printer.name) {
      this.cancelRename();
      return;
    }
    this.renamingFor.set(printer.queue_name);
    try {
      const msg = await this.tauri.renamePrinter(printer.queue_name, newName);
      this.renameResult.set({ ok: true, message: msg, printerName: printer.name });
      this.renamingPrinter.set(null);
      await this.refresh();
    } catch (e) {
      this.renameResult.set({ ok: false, message: String(e), printerName: printer.name });
    } finally {
      this.renamingFor.set(null);
    }
  }

  async printTestTcp(ip: string, size: ThermalSize): Promise<void> {
    const key = `${ip}::${size}`;
    this.tcpPrintingFor.set(key);
    this.tcpPrintResult.set(null);
    try {
      const result = await this.tauri.printTestTcp(ip, size);
      this.tcpPrintResult.set({ ok: true, message: result });
    } catch (e) {
      this.tcpPrintResult.set({ ok: false, message: String(e) });
    } finally {
      this.tcpPrintingFor.set(null);
    }
  }

  isTcpPrinting(ip: string, size: ThermalSize): boolean {
    return this.tcpPrintingFor() === `${ip}::${size}`;
  }

  async scanTcpIpPrinters(): Promise<void> {
    this.scanningPrinters.set(true);
    this.foundPrinters.set([]);
    try {
      const config = this.network.networkConfig();
      const ip = config?.ip ?? '192.168.1.1';
      const mask = config?.mask ?? '255.255.255.0';
      const printers = await this.tauri.scanTcpIpPrinters(ip, mask);
      this.foundPrinters.set(printers);
    } catch (e) {
      console.error('Error al escanear impresoras TCP/IP:', e);
    } finally {
      this.scanningPrinters.set(false);
    }
  }

  openAddUsbPrinterDialog(portName: string, deviceName: string): void {
    this.usbAddingFor.set(portName);
    this.usbPrinterNameInput.set(deviceName || portName);
    this.usbAddResult.set(null);
  }

  closeAddUsbPrinterDialog(): void {
    this.usbAddingFor.set(null);
    this.usbPrinterNameInput.set('');
    this.usbSavingPrinter.set(false);
  }

  async confirmAddUsbPrinter(): Promise<void> {
    const portName = this.usbAddingFor();
    const name = this.usbPrinterNameInput().trim();
    if (!portName || !name) return;

    this.usbSavingPrinter.set(true);
    this.usbAddResult.set(null);
    try {
      const result = await this.tauri.addUsbPrinter(portName, name);
      this.usbAddResult.set({ ok: true, message: result });
      this.closeAddUsbPrinterDialog();
      await new Promise(resolve => setTimeout(resolve, 1000));
      await this.refresh();
    } catch (e) {
      this.usbAddResult.set({ ok: false, message: String(e) });
      this.usbSavingPrinter.set(false);
    }
  }

  openAddPrinterDialog(ip: string): void {
    this.addingPrinter.set(ip);
    this.printerNameInput.set(`Impresora ${ip.split('.').pop()}`);
    this.printResult.set(null);
  }

  closeAddPrinterDialog(): void {
    this.addingPrinter.set(null);
    this.printerNameInput.set('');
    this.savingPrinter.set(false);
  }

  async confirmAddPrinter(): Promise<void> {
    const ip = this.addingPrinter();
    const name = this.printerNameInput().trim();
    if (!ip || !name) return;

    this.savingPrinter.set(true);
    this.printResult.set(null);
    try {
      const result = await this.tauri.addNetworkPrinter(ip, name);
      this.printResult.set({ ok: true, message: result });
      this.closeAddPrinterDialog();
      await new Promise(resolve => setTimeout(resolve, 1000));
      await this.refresh();
      this.foundPrinters.set([]);
    } catch (e) {
      this.printResult.set({ ok: false, message: String(e) });
      this.savingPrinter.set(false);
    }
  }
}
