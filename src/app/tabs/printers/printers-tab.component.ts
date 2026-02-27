import { Component, ChangeDetectionStrategy, viewChild, ElementRef, effect, inject } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { PrintersService, PrintSize } from './printers.service';
import { NetworkService } from '../network/network.service';

@Component({
  selector: 'app-printers-tab',
  imports: [NgIconComponent, CardComponent, BtnComponent],
  templateUrl: './printers-tab.component.html',
  styleUrl: './printers-tab.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PrintersTabComponent {
  protected readonly printers = inject(PrintersService);
  protected readonly network = inject(NetworkService);

  private readonly addPrinterDialog = viewChild<ElementRef<HTMLDialogElement>>('addPrinterDialog');

  constructor() {
    effect(() => {
      const dialog = this.addPrinterDialog()?.nativeElement;
      if (!dialog) return;
      if (this.printers.addingPrinter()) {
        if (!dialog.open) dialog.showModal();
      } else {
        if (dialog.open) dialog.close();
      }
    });
  }

  protected readonly printSizes: ReadonlyArray<{ key: PrintSize; label: string }> = [
    { key: 'a4', label: 'A4' },
    { key: 'thermal_58mm', label: '58 mm' },
    { key: 'thermal_80mm', label: '80 mm' },
  ];

  protected readonly thermalSizes: ReadonlyArray<{ key: import('./printers.service').ThermalSize; label: string }> = [
    { key: 'thermal_58mm', label: '58 mm' },
    { key: 'thermal_80mm', label: '80 mm' },
  ];
}
