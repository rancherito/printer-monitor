import { Component, OnInit, signal, computed, ChangeDetectionStrategy, inject } from '@angular/core';
import { TauriService, PrinterInfo, SystemInfo } from './tauri.service';

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
      const info = await this.tauri.getSystemInfo();
      this.systemInfo.set(info);
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
}
