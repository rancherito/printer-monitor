import { Component, OnInit, signal, computed, ChangeDetectionStrategy, inject } from '@angular/core';
import { TauriService, PrinterInfo, SystemInfo, NetworkDevice, BluetoothDevice, AppSettings } from './tauri.service';

type PrintSize = 'a4' | 'thermal_50mm' | 'thermal_80mm';

@Component({
  selector: 'app-root',
  imports: [],
  templateUrl: './app.html',
  styleUrl: './app.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App implements OnInit {
  private readonly tauri = inject(TauriService);

  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);
  protected readonly systemInfo = signal<SystemInfo | null>(null);
  protected readonly printingFor = signal<string | null>(null);
  protected readonly printResult = signal<{ ok: boolean; message: string } | null>(null);

  // Autostart
  protected readonly togglingAutostart = signal(false);
  protected readonly autostartEnabled = computed(() => this.systemInfo()?.autostart_enabled ?? false);

  // Configuración / puertos
  protected readonly settings = signal<AppSettings | null>(null);
  protected readonly isDev = computed(() => this.systemInfo()?.is_dev ?? false);
  protected readonly portDev = computed(() => this.settings()?.port_dev ?? 9002);
  protected readonly portProd = computed(() => this.settings()?.port_prod ?? 9003);
  protected readonly savingPort = signal(false);
  protected readonly portSaveResult = signal<{ ok: boolean; message: string } | null>(null);

  // Red
  protected readonly scanningNetwork = signal(false);
  protected readonly networkDevices = signal<NetworkDevice[]>([]);
  protected readonly networkError = signal<string | null>(null);

  // Bluetooth
  protected readonly loadingBluetooth = signal(false);
  protected readonly bluetoothDevices = signal<BluetoothDevice[]>([]);
  protected readonly bluetoothError = signal<string | null>(null);
  protected readonly bluetoothLoaded = signal(false);

  // Renombrar impresora
  protected readonly renamingPrinter = signal<string | null>(null);
  protected readonly renameValue = signal('');
  protected readonly renamingFor = signal<string | null>(null);
  protected readonly renameResult = signal<{ ok: boolean; message: string; printerName: string } | null>(null);

  protected readonly printers = computed(() => this.systemInfo()?.printers ?? []);
  protected readonly localIp = computed(() => this.systemInfo()?.local_ip ?? '—');
  protected readonly port = computed(() => this.systemInfo()?.port ?? '—');
  protected readonly frontendUrl = computed(() => {
    const ip = this.localIp();
    const p = this.port();
    return ip !== '—' ? `http://${ip}:${p}` : '—';
  });

  async ngOnInit(): Promise<void> {
    await this.refresh();
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
      this.settings.set(settings);
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.loading.set(false);
    }
  }

  async printTest(printer: PrinterInfo, size: PrintSize): Promise<void> {
    const key = `${printer.name}::${size}`;
    this.printingFor.set(key);
    this.printResult.set(null);
    try {
      const result = await this.tauri.printTest(printer.name, size);
      this.printResult.set({ ok: true, message: result });
    } catch (e) {
      this.printResult.set({ ok: false, message: String(e) });
    } finally {
      this.printingFor.set(null);
    }
  }

  isPrinting(printer: PrinterInfo, size: PrintSize): boolean {
    return this.printingFor() === `${printer.name}::${size}`;
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

  async scanNetwork(): Promise<void> {
    this.scanningNetwork.set(true);
    this.networkError.set(null);
    try {
      const devices = await this.tauri.scanNetwork();
      this.networkDevices.set(devices);
    } catch (e) {
      this.networkError.set(String(e));
    } finally {
      this.scanningNetwork.set(false);
    }
  }

  async loadBluetooth(): Promise<void> {
    this.loadingBluetooth.set(true);
    this.bluetoothError.set(null);
    this.bluetoothLoaded.set(false);
    try {
      const devices = await this.tauri.getBluetoothDevices();
      this.bluetoothDevices.set(devices);
      this.bluetoothLoaded.set(true);
    } catch (e) {
      this.bluetoothError.set(String(e));
    } finally {
      this.loadingBluetooth.set(false);
    }
  }

  async savePort(key: 'port_dev' | 'port_prod', value: string): Promise<void> {
    const port = parseInt(value, 10);
    if (isNaN(port) || port < 1 || port > 65535) {
      this.portSaveResult.set({ ok: false, message: 'Puerto inválido (1-65535)' });
      return;
    }
    this.savingPort.set(true);
    this.portSaveResult.set(null);
    try {
      await this.tauri.setSetting(key, String(port));
      const updated = await this.tauri.getSettings();
      this.settings.set(updated);
      this.portSaveResult.set({ ok: true, message: `Puerto ${key === 'port_dev' ? 'desarrollo' : 'producción'} actualizado a ${port}` });
    } catch (e) {
      this.portSaveResult.set({ ok: false, message: String(e) });
    } finally {
      this.savingPort.set(false);
    }
  }

  startRename(printer: PrinterInfo): void {
    this.renamingPrinter.set(printer.name);
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
    this.renamingFor.set(printer.name);
    try {
      const msg = await this.tauri.renamePrinter(printer.name, newName);
      this.renameResult.set({ ok: true, message: msg, printerName: printer.name });
      this.renamingPrinter.set(null);
      await this.refresh();
    } catch (e) {
      this.renameResult.set({ ok: false, message: String(e), printerName: printer.name });
    } finally {
      this.renamingFor.set(null);
    }
  }
}
