import {
  ChangeDetectionStrategy,
  Component,
  inject,
  signal,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { NgIcon } from '@ng-icons/core';
import { BtnComponent } from '../btn.component';
import { CardComponent } from '../card.component';
import { SystemResourceService } from '../core/system-resource.service';
import { PrintersResourceService } from '../core/printers-resource.service';
import { LogService } from '../services/log.service';

@Component({
  selector: 'app-home',
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './home.component.html',
  styleUrl: './home.component.scss',
  imports: [FormsModule, BtnComponent, CardComponent, NgIcon],
})
export class HomeComponent {
  readonly system = inject(SystemResourceService);
  readonly printers = inject(PrintersResourceService);
  readonly logService = inject(LogService);

  readonly activeTab = signal<'printers' | 'config' | 'logs'>('printers');

  readonly renameTarget = signal<string | null>(null);
  readonly renameValue = signal('');

  readonly editingServerPort = signal(false);
  readonly serverPortInput = signal('');

  readonly editingOutputDir = signal(false);
  readonly outputDirInput = signal('');

  refresh(): void {
    this.system.refreshAll();
    this.printers.refresh();
  }

  switchTab(tab: 'printers' | 'config' | 'logs'): void {
    this.activeTab.set(tab);
    if (tab === 'config') {
      this.system.refreshAll();
    }
    if (tab === 'logs') {
      this.logService.markAllRead();
    }
  }

  startRename(printer: { queue_name: string; name: string }): void {
    this.renameTarget.set(printer.queue_name);
    this.renameValue.set(printer.name);
  }

  cancelRename(): void {
    this.renameTarget.set(null);
    this.renameValue.set('');
  }

  async confirmRename(): Promise<void> {
    const target = this.renameTarget();
    if (!target) return;
    await this.printers.renamePrinter(target, this.renameValue());
    this.renameTarget.set(null);
  }

  openPortEditor(): void {
    this.serverPortInput.set(String(this.system.port()));
    this.editingServerPort.set(true);
  }

  closePortEditor(): void {
    this.editingServerPort.set(false);
  }

  async saveServerPort(): Promise<void> {
    const port = parseInt(this.serverPortInput(), 10);
    if (isNaN(port) || port < 1 || port > 65535) return;
    await this.system.setServerPort(port);
    this.editingServerPort.set(false);
  }

  openDirEditor(): void {
    this.outputDirInput.set(this.system.outputDir.value() ?? '');
    this.editingOutputDir.set(true);
  }

  closeDirEditor(): void {
    this.editingOutputDir.set(false);
  }

  async saveOutputDir(): Promise<void> {
    await this.system.saveOutputDir(this.outputDirInput());
    this.editingOutputDir.set(false);
  }

  formatDate(ms: number): string {
    return new Date(ms).toLocaleString();
  }

  formatTime(d: Date): string {
    const now = new Date();
    const isToday = d.toDateString() === now.toDateString();
    if (isToday) {
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    }
    return (
      d.toLocaleDateString([], { month: 'short', day: 'numeric' }) +
      ' ' +
      d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    );
  }
}
