import { Component, input, output, signal, ChangeDetectionStrategy, viewChild, ElementRef, effect } from '@angular/core';
import { NgIconComponent } from '@ng-icons/core';
import { CardComponent } from '../../card.component';
import { BtnComponent } from '../../btn.component';
import { IpInputComponent } from '../../ip-input.component';
import { PrinterInfo, NetworkConfig } from '../../tauri.service';

type PrintSize = 'a4' | 'thermal_50mm' | 'thermal_80mm';

@Component({
  selector: 'app-printers-tab',
  standalone: true,
  imports: [NgIconComponent, CardComponent, BtnComponent, IpInputComponent],
  templateUrl: './printers-tab.component.html',
  styleUrl: './printers-tab.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PrintersTabComponent {
  readonly loading = input.required<boolean>();
  readonly printers = input.required<PrinterInfo[]>();
  readonly printingFor = input.required<string | null>();
  readonly printResult = input.required<{ ok: boolean; message: string } | null>();
  readonly renamingPrinter = input.required<string | null>();
  readonly renameValue = input.required<string>();
  readonly renamingFor = input.required<string | null>();
  readonly renameResult = input.required<{ ok: boolean; message: string; printerName: string } | null>();
  readonly customIp = input.required<string>();
  readonly customMask = input.required<string>();
  readonly scanningPrinters = input.required<boolean>();
  readonly foundPrinters = input.required<string[]>();
  
  // Network config
  readonly networkConfig = input.required<NetworkConfig | null>();
  readonly loadingNetworkConfig = input.required<boolean>();
  readonly savingNetworkConfig = input.required<boolean>();
  readonly networkConfigResult = input.required<{ ok: boolean; message: string } | null>();
  readonly editingNetworkConfig = input.required<boolean>();
  readonly tempIp = input.required<string>();
  readonly tempMask = input.required<string>();
  readonly tempGateway = input.required<string>();
  
  // Adding printer
  readonly addingPrinter = input.required<string | null>();
  readonly printerNameInput = input.required<string>();
  readonly savingPrinter = input.required<boolean>();

  // Dialog ref
  private readonly addPrinterDialog = viewChild<ElementRef<HTMLDialogElement>>('addPrinterDialog');

  constructor() {
    // Abrir/cerrar dialog reactivamente
    effect(() => {
      const dialog = this.addPrinterDialog()?.nativeElement;
      if (!dialog) return;
      
      if (this.addingPrinter()) {
        if (!dialog.open) {
          dialog.showModal();
        }
      } else {
        if (dialog.open) {
          dialog.close();
        }
      }
    });
  }

  readonly printTestClick = output<{ printer: PrinterInfo; size: PrintSize }>();
  readonly startRenameClick = output<PrinterInfo>();
  readonly cancelRenameClick = output<void>();
  readonly confirmRenameClick = output<PrinterInfo>();
  readonly renameValueChange = output<string>();
  readonly customIpChange = output<string>();
  readonly customMaskChange = output<string>();
  readonly scanTcpIpPrintersClick = output<void>();
  
  // Network config outputs
  readonly loadNetworkConfigClick = output<void>();
  readonly startEditNetworkConfigClick = output<void>();
  readonly cancelEditNetworkConfigClick = output<void>();
  readonly saveNetworkConfigClick = output<void>();
  readonly restoreNetworkDhcpClick = output<void>();
  readonly tempIpChange = output<string>();
  readonly tempMaskChange = output<string>();
  readonly tempGatewayChange = output<string>();
  
  // Adding printer outputs
  readonly openAddPrinterDialogClick = output<string>();
  readonly closeAddPrinterDialogClick = output<void>();
  readonly printerNameInputChange = output<string>();
  readonly confirmAddPrinterClick = output<void>();

  protected readonly printSizes: ReadonlyArray<{ key: PrintSize; label: string }> = [
    { key: 'a4', label: 'A4' },
    { key: 'thermal_50mm', label: '50 mm' },
    { key: 'thermal_80mm', label: '80 mm' },
  ];

  isPrinting(printer: PrinterInfo, size: PrintSize): boolean {
    return this.printingFor() === `${printer.queue_name}::${size}`;
  }
}
