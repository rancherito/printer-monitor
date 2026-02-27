import { Component, ChangeDetectionStrategy, viewChild, ElementRef, effect, inject, computed } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { PrintersService, PrintSize } from './printers.service';
import { PdfPrintWidth } from '../../services/pdf.service';
import { NetworkService } from '../network/network.service';
import { DomSanitizer, SafeResourceUrl } from '@angular/platform-browser';

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
  private readonly sanitizer = inject(DomSanitizer);

  /** URL segura del blob PDF para usar en [src] del iframe. */
  protected readonly safePdfUrl = computed<SafeResourceUrl | null>(() => {
    const url = this.printers.pdfPreviewUrl();
    return url ? this.sanitizer.bypassSecurityTrustResourceUrl(url) : null;
  });

  private readonly addPrinterDialog = viewChild<ElementRef<HTMLDialogElement>>('addPrinterDialog');
  private readonly addUsbPrinterDialog = viewChild<ElementRef<HTMLDialogElement>>('addUsbPrinterDialog');

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

    effect(() => {
      const dialog = this.addUsbPrinterDialog()?.nativeElement;
      if (!dialog) return;
      if (this.printers.usbAddingFor()) {
        if (!dialog.open) dialog.showModal();
      } else {
        if (dialog.open) dialog.close();
      }
    });
  }

  protected readonly printSizes: ReadonlyArray<{ key: PrintSize; label: string }> = [
    { key: 'thermal_58mm', label: '58 mm' },
    { key: 'thermal_80mm', label: '80 mm' },
  ];

  protected readonly thermalSizes: ReadonlyArray<{ key: import('./printers.service').ThermalSize; label: string }> = [
    { key: 'thermal_58mm', label: '58 mm' },
    { key: 'thermal_80mm', label: '80 mm' },
  ];

  protected readonly pdfWidths: ReadonlyArray<{ key: PdfPrintWidth; label: string }> = [
    { key: '58mm', label: '58 mm' },
    { key: '80mm', label: '80 mm' },
  ];
}
