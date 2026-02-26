import {
  Component,
  signal,
  output,
  input,
  effect,
  untracked,
  ChangeDetectionStrategy,
  viewChildren,
  ElementRef,
} from '@angular/core';

/**
 * Input de dirección IP dividido en 4 octetos (###.###.###.###).
 * Navega automáticamente entre campos y valida 0-255 por octeto.
 */
@Component({
  selector: 'app-ip-input',
  template: `
    <div class="flex items-center gap-1" role="group" >     
      @for (octet of octets(); track $index) {
        <input
          type="text"
          inputmode="numeric"
          maxlength="3"
          class="w-14 h-9 px-2 text-center rounded-md border border-slate-300 text-sm font-mono text-slate-900 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent disabled:opacity-50 disabled:bg-slate-50"
          [value]="octet"
          (input)="onInput($index, $any($event.target).value)"
          (keydown)="onKeyDown($index, $event)"
          (paste)="onPaste($event)"
          [disabled]="disabled()"
          [attr.aria-label]="'Octeto ' + ($index + 1)"
          #octetInput />
        @if ($index < 3) {
          <span class="text-slate-400 font-bold select-none">.</span>
        }
      }
    </div>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class IpInputComponent {
  readonly value = input<string>('0.0.0.0');
  readonly disabled = input(false);
  readonly valueChange = output<string>();

  protected readonly octets = signal<string[]>(['0', '0', '0', '0']);

  constructor() {
    effect(() => {
      const ip = this.value();
      const parts = ip.split('.');
      if (parts.length === 4) {
        this.octets.set(parts.map(p => parseInt(p, 10).toString()));
      }
    });
  }

  protected onInput(index: number, value: string): void {
    const cleaned = value.replace(/\D/g, '').slice(0, 3);
    const num = parseInt(cleaned || '0', 10);
    const clamped = Math.min(255, num).toString();

    this.octets.update(arr => {
      const copy = [...arr];
      copy[index] = clamped;
      return copy;
    });

    this.valueChange.emit(this.octets().join('.'));

    // Auto-avanzar al siguiente input si se llena
    if (cleaned.length === 3 && index < 3) {
      const inputs = document.querySelectorAll('input[type="text"]');
      (inputs[index + 1] as HTMLInputElement)?.focus();
    }
  }

  protected onKeyDown(index: number, event: KeyboardEvent): void {
    const input = event.target as HTMLInputElement;
    const value = input.value;

    // Backspace en campo vacío → mover al anterior
    if (event.key === 'Backspace' && value === '' && index > 0) {
      event.preventDefault();
      const inputs = document.querySelectorAll('input[type="text"]');
      (inputs[index - 1] as HTMLInputElement)?.focus();
    }

    // Punto → mover al siguiente
    if (event.key === '.' && index < 3) {
      event.preventDefault();
      const inputs = document.querySelectorAll('input[type="text"]');
      (inputs[index + 1] as HTMLInputElement)?.focus();
    }

    // Flecha derecha al final → siguiente
    if (event.key === 'ArrowRight' && input.selectionStart === value.length && index < 3) {
      event.preventDefault();
      const inputs = document.querySelectorAll('input[type="text"]');
      (inputs[index + 1] as HTMLInputElement)?.focus();
    }

    // Flecha izquierda al inicio → anterior
    if (event.key === 'ArrowLeft' && input.selectionStart === 0 && index > 0) {
      event.preventDefault();
      const inputs = document.querySelectorAll('input[type="text"]');
      (inputs[index - 1] as HTMLInputElement)?.focus();
    }
  }

  protected onPaste(event: ClipboardEvent): void {
    event.preventDefault();
    const text = event.clipboardData?.getData('text') || '';
    const match = text.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
    
    if (match) {
      const parts = [match[1], match[2], match[3], match[4]];
      const clamped = parts.map(p => Math.min(255, parseInt(p, 10)).toString());
      this.octets.set(clamped);
      this.valueChange.emit(this.octets().join('.'));
    }
  }
}
