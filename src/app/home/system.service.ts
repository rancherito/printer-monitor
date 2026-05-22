import { Injectable, signal, computed } from '@angular/core';
import { TauriService } from '../services/tauri.service';
import { SystemInfo } from '../models';

@Injectable({ providedIn: 'root' })
export class SystemService {
  private readonly tauri = new TauriService();

  readonly systemInfo = signal<SystemInfo | null>(null);
  readonly loading = signal(false);
  readonly error = signal<string | null>(null);
  /** Puerto guardado pendiente de aplicar (requiere reinicio) */
  readonly pendingPort = signal<number | null>(null);

  readonly localIp = computed(() => this.systemInfo()?.local_ip ?? '—');
  readonly isDev = computed(() => this.systemInfo()?.is_dev ?? false);
  readonly autostartEnabled = computed(() => this.systemInfo()?.autostart_enabled ?? false);
  readonly serverPort = computed(() => this.systemInfo()?.port ?? 8001);
  readonly serverAddress = computed(() => `${this.localIp()}:${this.serverPort()}`);

  async loadSystemInfo(): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      const info = await this.tauri.getSystemInfo();
      this.systemInfo.set(info);
    } catch (e) {
      this.error.set(String(e));
    } finally {
      this.loading.set(false);
    }
  }

  async toggleAutostart(): Promise<void> {
    const current = this.autostartEnabled();
    try {
      await this.tauri.setAutostartEnabled(!current);
      this.systemInfo.update(s => s ? { ...s, autostart_enabled: !current } : s);
    } catch (e) {
      this.error.set(String(e));
    }
  }

  async setServerPort(port: number): Promise<void> {
    try {
      await this.tauri.setServerPort(port);
      this.pendingPort.set(port);
    } catch (e) {
      this.error.set(String(e));
    }
  }
}
