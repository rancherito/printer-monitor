import { Component, OnInit, OnDestroy, signal, computed, ChangeDetectionStrategy, inject, ElementRef, viewChild } from '@angular/core';
import { listen } from '@tauri-apps/api/event';
import { NgIconComponent } from '@ng-icons/core';
import { BtnComponent } from './btn.component';
import { DashboardTabComponent } from './tabs/dashboard/dashboard-tab.component';
import { PrintersTabComponent } from './tabs/printers/printers-tab.component';
import { NetworkTabComponent } from './tabs/network/network-tab.component';
import { BluetoothTabComponent } from './tabs/bluetooth-tab/bluetooth-tab.component';
import { TauriService, PrinterInfo, SystemInfo, NetworkDevice, BluetoothDevice, AppSettings, SerialPort, NetworkConfig } from './tauri.service';

type PrintSize = 'a4' | 'thermal_50mm' | 'thermal_80mm';
type TabId = 'dashboard' | 'printers' | 'network' | 'bluetooth';

@Component({
  selector: 'app-root',
  imports: [NgIconComponent, BtnComponent, DashboardTabComponent, PrintersTabComponent, NetworkTabComponent, BluetoothTabComponent],
  templateUrl: './app.html',
  styleUrl: './app.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class App implements OnInit, OnDestroy {
  private readonly tauri = inject(TauriService);
  private unlistenPrinters: (() => void) | null = null;

  protected readonly tabs: ReadonlyArray<{ id: TabId; label: string; icon: string }> = [
    { id: 'dashboard', label: 'Dashboard', icon: 'matDashboard' },
    { id: 'printers', label: 'Impresoras', icon: 'matPrint' },
    { id: 'network', label: 'Red', icon: 'matLan' },
    { id: 'bluetooth', label: 'Bluetooth', icon: 'matBluetooth' },
  ];

  // Navegación por tabs
  protected readonly activeTab = signal<TabId>('dashboard');

  protected readonly printSizes: ReadonlyArray<{ key: PrintSize; label: string }> = [
    { key: 'a4', label: 'A4' },
    { key: 'thermal_50mm', label: '50 mm' },
    { key: 'thermal_80mm', label: '80 mm' },
  ];

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

  // Configuración IP personalizada
  protected readonly customIp = signal('192.168.1.100');
  protected readonly customMask = signal('255.255.255.0');
  protected readonly scanningPrinters = signal(false);
  protected readonly foundPrinters = signal<string[]>([]);

  // Configuración de red del equipo
  protected readonly networkConfig = signal<NetworkConfig | null>(null);
  protected readonly loadingNetworkConfig = signal(false);
  protected readonly savingNetworkConfig = signal(false);
  protected readonly networkConfigResult = signal<{ ok: boolean; message: string } | null>(null);
  protected readonly editingNetworkConfig = signal(false);
  protected readonly tempIp = signal('');
  protected readonly tempMask = signal('');
  protected readonly tempGateway = signal('');

  // Agregar impresora
  protected readonly addingPrinter = signal<string | null>(null);
  protected readonly printerNameInput = signal('');
  protected readonly savingPrinter = signal(false);

  protected readonly printers = computed(() => this.systemInfo()?.printers ?? []);
  protected readonly serialPorts = computed(() => this.systemInfo()?.serial_ports ?? []);
  protected readonly localIp = computed(() => this.systemInfo()?.local_ip ?? '—');
  protected readonly port = computed(() => this.systemInfo()?.port ?? '—');
  protected readonly frontendUrl = computed(() => {
    const ip = this.localIp();
    const p = this.port();
    return ip !== '—' ? `http://${ip}:${p}` : '—';
  });

  async ngOnInit(): Promise<void> {
    // Suscripción permanente: el backend emite 'printers-updated' cada vez que
    // detecta un cambio en impresoras o puertos USB/COM (watcher cada 2 s)
    this.unlistenPrinters = await listen<{ printers: PrinterInfo[]; serial_ports: SerialPort[] }>(
      'printers-updated',
      ({ payload }) => {
        this.systemInfo.update(info =>
          info ? { ...info, printers: payload.printers, serial_ports: payload.serial_ports } : info
        );
      },
    );
    await this.refresh();
    await this.loadNetworkConfig();
  }

  ngOnDestroy(): void {
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

  setActiveTab(tab: TabId): void {
    this.activeTab.set(tab);
  }

  async scanTcpIpPrinters(): Promise<void> {
    this.scanningPrinters.set(true);
    this.foundPrinters.set([]);
    try {
      // Usar la IP actual del equipo si está disponible
      const ip = this.networkConfig()?.ip || this.localIp();
      const mask = this.networkConfig()?.mask || '255.255.255.0';
      const printers = await this.tauri.scanTcpIpPrinters(ip, mask);
      this.foundPrinters.set(printers);
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.scanningPrinters.set(false);
    }
  }

  async loadNetworkConfig(): Promise<void> {
    this.loadingNetworkConfig.set(true);
    try {
      const config = await this.tauri.getNetworkConfig();
      this.networkConfig.set(config);
      this.customIp.set(config.ip);
      this.customMask.set(config.mask);
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.loadingNetworkConfig.set(false);
    }
  }

  startEditNetworkConfig(): void {
    const config = this.networkConfig();
    if (config) {
      this.tempIp.set(config.ip);
      this.tempMask.set(config.mask);
      this.tempGateway.set(config.gateway);
      this.editingNetworkConfig.set(true);
      this.networkConfigResult.set(null);
    }
  }

  cancelEditNetworkConfig(): void {
    this.editingNetworkConfig.set(false);
    this.tempIp.set('');
    this.tempMask.set('');
    this.tempGateway.set('');
  }

  async saveNetworkConfig(): Promise<void> {
    this.savingNetworkConfig.set(true);
    this.networkConfigResult.set(null);
    try {
      const result = await this.tauri.setNetworkConfig(
        this.tempIp(),
        this.tempMask(),
        this.tempGateway()
      );
      this.networkConfigResult.set({ ok: true, message: result });
      // Actualizar la señal inmediatamente con los valores guardados
      // para que la UI los muestre de forma instantánea y correcta.
      const currentConfig = this.networkConfig();
      this.networkConfig.set({
        ip: this.tempIp(),
        mask: this.tempMask(),
        gateway: this.tempGateway(),
        interface: currentConfig?.interface ?? '',
      });
      this.editingNetworkConfig.set(false);
      // Recargar desde el SO tras un tiempo prudencial para confirmar
      setTimeout(() => this.loadNetworkConfig(), 3000);
    } catch (e) {
      this.networkConfigResult.set({ ok: false, message: String(e) });
    } finally {
      this.savingNetworkConfig.set(false);
    }
  }

  async restoreNetworkDhcp(): Promise<void> {
    this.savingNetworkConfig.set(true);
    this.networkConfigResult.set(null);
    try {
      const result = await this.tauri.restoreNetworkDhcp();
      this.networkConfigResult.set({ ok: true, message: result });
      // Recargar configuración después de restaurar
      setTimeout(() => this.loadNetworkConfig(), 2000);
    } catch (e) {
      this.networkConfigResult.set({ ok: false, message: String(e) });
    } finally {
      this.savingNetworkConfig.set(false);
    }
  }

  openAddPrinterDialog(ip: string): void {
    this.addingPrinter.set(ip);
    this.printerNameInput.set(`Impresora ${ip.split('.').pop()}`);
    this.printResult.set(null); // Limpiar resultado anterior
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
      console.log(`🖨️ Agregando impresora: ${name} en ${ip}`);
      const result = await this.tauri.addNetworkPrinter(ip, name);
      console.log(`✅ Éxito:`, result);
      
      this.printResult.set({ ok: true, message: result });
      this.closeAddPrinterDialog();
      
      // Esperar un momento antes de refrescar para que el sistema registre la impresora
      await new Promise(resolve => setTimeout(resolve, 1000));
      
      // Refrescar lista de impresoras
      await this.refresh();
      
      // Limpiar impresoras encontradas
      this.foundPrinters.set([]);
    } catch (e) {
      console.error(`❌ Error al agregar impresora:`, e);
      this.printResult.set({ ok: false, message: String(e) });
      this.savingPrinter.set(false);
    }
  }
}
