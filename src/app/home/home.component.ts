import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  signal,
  inject,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { BtnComponent } from '../btn.component';
import { CardComponent } from '../card.component';
import { SystemService } from './system.service';
import { PrintersService } from './printers.service';
import { PrinterInfo } from '../models';

@Component({
  selector: 'app-home',
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './home.component.html',
  styleUrl: './home.component.scss',
  imports: [FormsModule, BtnComponent, CardComponent],
})
export class HomeComponent implements OnInit {
  readonly system = inject(SystemService);
  readonly printers = inject(PrintersService);

  readonly renameTarget = signal<string | null>(null);
  readonly renameValue = signal('');

  readonly editingServerPort = signal(false);
  readonly serverPortInput = signal('');

  async ngOnInit(): Promise<void> {
    await this.system.loadSystemInfo();
    await this.printers.loadPrinters();
  }

  async refresh(): Promise<void> {
    await this.system.loadSystemInfo();
    await this.printers.loadPrinters();
  }

  startRename(printer: PrinterInfo): void {
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
    this.serverPortInput.set(String(this.system.serverPort()));
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
}
