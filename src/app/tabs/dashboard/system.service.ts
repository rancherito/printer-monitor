import { Injectable, inject, signal, computed } from '@angular/core';
import { listen } from '@tauri-apps/api/event';
import { TauriService, SystemInfo, PrinterInfo, SerialPort } from '../../services/tauri.service';
import { PrintersService } from '../printers/printers.service';
import { NetworkService } from '../network/network.service';

type UsbPrintSize = 'thermal_58mm' | 'thermal_80mm';

@Injectable({ providedIn: 'root' })
export class SystemService {
  private readonly tauri = inject(TauriService);
  private readonly printersService = inject(PrintersService);
  private readonly networkService = inject(NetworkService);

  private unlistenPrinters: (() => void) | null = null;

  readonly loading = signal(true);
  readonly error = signal<string | null>(null);
  readonly systemInfo = signal<SystemInfo | null>(null);
  readonly togglingAutostart = signal(false);
  readonly usbPrintingFor = signal<string | null>(null);
  readonly usbPrintResult = signal<{ ok: boolean; message: string } | null>(null);

  readonly isDev = computed(() => this.systemInfo()?.is_dev ?? false);
  readonly localIp = computed(() => this.systemInfo()?.local_ip ?? '—');
  readonly port = computed((): number | '—' => this.systemInfo()?.port ?? '—');
  readonly frontendUrl = computed(() => {
    const ip = this.localIp();
    const p = this.port();
    return ip !== '—' ? `http://${ip}:${p}` : '—';
  });
  readonly autostartEnabled = computed(() => this.systemInfo()?.autostart_enabled ?? false);

  async init(): Promise<void> {
    this.unlistenPrinters = await listen<{ printers: PrinterInfo[]; serial_ports: SerialPort[] }>(
      'printers-updated',
      ({ payload }) => {
        this.systemInfo.update(info =>
          info ? { ...info, printers: payload.printers, serial_ports: payload.serial_ports } : info
        );
        this.printersService.updateFromSystemInfo(payload.printers, payload.serial_ports);
      },
    );
    await this.refresh();
    await this.networkService.loadNetworkConfig();
  }

  destroy(): void {
    this.unlistenPrinters?.();
  }

  async refresh(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      const [info, settings] = await Promise.all([
        this.tauri.getSystemInfo(),
        this.tauri.getSettings(),
      ]);
      this.systemInfo.set(info);
      this.printersService.updateFromSystemInfo(info.printers, info.serial_ports);
      this.networkService.settings.set(settings);
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.loading.set(false);
    }
  }

  async toggleAutostart(): Promise<void> {
    this.togglingAutostart.set(true);
    try {
      const newVal = !this.autostartEnabled();
      await this.tauri.setAutostartEnabled(newVal);
      this.systemInfo.update(info =>
        info ? { ...info, autostart_enabled: newVal } : info
      );
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.togglingAutostart.set(false);
    }
  }

  async printTestUsb(portName: string, size: UsbPrintSize): Promise<void> {
    const key = `${portName}::${size}`;
    this.usbPrintingFor.set(key);
    this.usbPrintResult.set(null);
    try {
      const result = await this.tauri.printTestUsb(portName, size);
      this.usbPrintResult.set({ ok: true, message: result });
    } catch (e) {
      this.usbPrintResult.set({ ok: false, message: String(e) });
    } finally {
      this.usbPrintingFor.set(null);
    }
  }
}
