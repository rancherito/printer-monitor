import { Injectable, inject, signal, computed } from '@angular/core';
import { TauriService, NetworkConfig, NetworkDevice, AppSettings } from '../../services/tauri.service';

@Injectable({ providedIn: 'root' })
export class NetworkService {
  private readonly tauri = inject(TauriService);

  // ── Configuración de red del equipo ────────────────────────────────────
  readonly networkConfig = signal<NetworkConfig | null>(null);
  readonly loadingNetworkConfig = signal(false);
  readonly savingNetworkConfig = signal(false);
  readonly networkConfigResult = signal<{ ok: boolean; message: string } | null>(null);
  readonly editingNetworkConfig = signal(false);
  readonly tempIp = signal('');
  readonly tempMask = signal('');
  readonly tempGateway = signal('');

  // ── Escaneo de dispositivos en la subred ───────────────────────────────
  readonly scanningNetwork = signal(false);
  readonly networkDevices = signal<NetworkDevice[]>([]);
  readonly networkError = signal<string | null>(null);

  // ── Configuración de puertos del servidor ──────────────────────────────
  readonly settings = signal<AppSettings | null>(null);
  readonly savingPort = signal(false);
  readonly portSaveResult = signal<{ ok: boolean; message: string } | null>(null);

  readonly portDev = computed(() => this.settings()?.port_dev ?? 9002);
  readonly portProd = computed(() => this.settings()?.port_prod ?? 9003);

  async loadNetworkConfig(): Promise<void> {
    this.loadingNetworkConfig.set(true);
    try {
      const config = await this.tauri.getNetworkConfig();
      this.networkConfig.set(config);
    } catch (e) {
      console.error('Error al obtener configuración de red:', e);
    } finally {
      this.loadingNetworkConfig.set(false);
    }
  }

  startEditNetworkConfig(): void {
    const config = this.networkConfig();
    if (!config) return;
    this.tempIp.set(config.ip);
    this.tempMask.set(config.mask);
    this.tempGateway.set(config.gateway);
    this.editingNetworkConfig.set(true);
    this.networkConfigResult.set(null);
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
        this.tempGateway(),
      );
      this.networkConfigResult.set({ ok: true, message: result });
      this.networkConfig.set({
        ip: this.tempIp(),
        mask: this.tempMask(),
        gateway: this.tempGateway(),
        interface: this.networkConfig()?.interface ?? '',
      });
      this.editingNetworkConfig.set(false);
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
      setTimeout(() => this.loadNetworkConfig(), 2000);
    } catch (e) {
      this.networkConfigResult.set({ ok: false, message: String(e) });
    } finally {
      this.savingNetworkConfig.set(false);
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

  async loadSettings(): Promise<void> {
    try {
      const settings = await this.tauri.getSettings();
      this.settings.set(settings);
    } catch (e) {
      console.error('Error al cargar ajustes:', e);
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
      this.portSaveResult.set({
        ok: true,
        message: `Puerto ${key === 'port_dev' ? 'desarrollo' : 'producción'} actualizado a ${port}`,
      });
    } catch (e) {
      this.portSaveResult.set({ ok: false, message: String(e) });
    } finally {
      this.savingPort.set(false);
    }
  }
}
