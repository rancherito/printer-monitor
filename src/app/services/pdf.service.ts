import { Injectable, inject } from '@angular/core';
import { TauriService } from './tauri.service';

/** Ancho de impresión: determina el escalado de imagen en Rust (384 px / 576 px). */
export type PdfPrintWidth = '58mm' | '80mm';

/** Genera documentos PDF con pdfmake y los envía a imprimir vía Tauri. */
@Injectable({ providedIn: 'root' })
export class PdfService {
  private readonly tauri = inject(TauriService);

  // ── Generación de documentos ──────────────────────────────────────────

  /**
   * Devuelve el docDefinition de pdfmake para un ticket de prueba (80 mm).
   * La anchura del papel en pdfmake se fija a 226.77 pt (= 80 mm).
   * Rust se encarga de escalar la imagen al ancho real de la impresora (58/80 mm).
   */
  testDocDef(printerName: string): object {
    const now = new Date();
    const dateStr = now.toLocaleDateString('es-ES');
    const timeStr = now.toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' });
    const LINE_W = 216.77; // ancho imprimible: 226.77 − 5 pt × 2

    return {
      pageSize: { width: 226.77, height: 'auto' },
      pageMargins: [5, 10, 5, 10],
      content: [
        { text: 'PRINTER MONITOR', style: 'header' },
        { text: 'CODICORE', style: 'subheader' },
        { canvas: [{ type: 'line', x1: 0, y1: 0, x2: LINE_W, y2: 0, lineWidth: 0.5 }] },
        { text: ' ' },
        { text: 'Prueba de impresión PDF', fontSize: 9, alignment: 'center' },
        { text: ' ' },
        { canvas: [{ type: 'line', x1: 0, y1: 0, x2: LINE_W, y2: 0, lineWidth: 0.5 }] },
        { text: ' ' },
        { columns: [{ text: 'Impresora:', width: 70, fontSize: 8, bold: true }, { text: printerName, fontSize: 8 }] },
        { columns: [{ text: 'Fecha:', width: 70, fontSize: 8, bold: true }, { text: dateStr, fontSize: 8 }], margin: [0, 2, 0, 0] },
        { columns: [{ text: 'Hora:', width: 70, fontSize: 8, bold: true }, { text: timeStr, fontSize: 8 }], margin: [0, 2, 0, 0] },
        { text: ' ' },
        { canvas: [{ type: 'line', x1: 0, y1: 0, x2: LINE_W, y2: 0, lineWidth: 0.5 }] },
        { text: ' ' },
        { text: '¡Si ves esto, funciona!', fontSize: 10, bold: true, alignment: 'center' },
        { text: ' ' },
      ],
      styles: {
        header:    { fontSize: 14, bold: true,  alignment: 'center', marginBottom: 2 },
        subheader: { fontSize: 10, bold: false, alignment: 'center', marginBottom: 6, color: '#555' },
      },
    };
  }

  // ── Conversión ────────────────────────────────────────────────────────

  /**
   * Convierte un docDefinition de pdfmake a PDF en base64.
   * Carga pdfmake y las fuentes de forma lazy (no bloquea el bundle principal).
   */
  async toPdfBase64(docDef: object): Promise<string> {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const pdfMakeModule: any = await import('pdfmake/build/pdfmake');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const pdfFontsModule: any = await import('pdfmake/build/vfs_fonts');

    const pdfMake = pdfMakeModule.default ?? pdfMakeModule;
    const pdfFonts = pdfFontsModule.default ?? pdfFontsModule;

    // vfs_fonts exporta { vfs: { 'Roboto-Regular.ttf': '...b64...', ... } }
    pdfMake.vfs = pdfFonts.vfs;

    // pdfmake 0.3.x: getBase64() devuelve Promise<string> (ya no usa callback)
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return (pdfMake.createPdf(docDef as any) as any).getBase64();
  }

  // ── Impresión ─────────────────────────────────────────────────────────

  /**
   * Genera el ticket de prueba PDF y lo envía a Rust vía Tauri IPC.
   * Rust convierte el PDF a imagen (sips) y lo imprime usando ESC*.
   */
  async printTestPdf(printerName: string, width: '58mm'): Promise<string> {
    const docDef = this.testDocDef(printerName);
    const pdfB64 = await this.toPdfBase64(docDef);
    return this.tauri.printPdf(pdfB64, printerName, width);
  }

  /**
   * Imprime cualquier PDF (ya en base64) en la impresora indicada.
   * Punto de entrada principal para integración futura con PDFs recibidos por API.
   */
  async printPdfBase64(pdfB64: string, printerName: string, width: '58mm'): Promise<string> {
    return this.tauri.printPdf(pdfB64, printerName, width);
  }
}
