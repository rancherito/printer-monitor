import { Component, input, computed, ChangeDetectionStrategy } from '@angular/core';

/**
 * Botón reutilizable con variantes y estado de carga.
 * Usa selector de atributo sobre <button> para preservar toda la semántica nativa
 * (type, disabled, aria-*, etc.) sin wrappers extra.
 *
 * Usage:
 *   <button appBtn>Guardar</button>
 *   <button appBtn variant="primary" [loading]="saving()">Guardar</button>
 *   <button appBtn variant="ghost" size="sm">Cancelar</button>
 *   <button appBtn variant="danger">Eliminar</button>
 */
@Component({
  selector: 'button[appBtn]',
  template: `
    @if (loading()) {
      <span
        class="inline-block w-3.5 h-3.5 border-2 border-current border-r-transparent rounded-full animate-spin shrink-0"
        aria-hidden="true"
      ></span>
    }
    <ng-content></ng-content>
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '[class]': 'cls()' },
})
export class BtnComponent {
  readonly variant = input<'primary' | 'secondary' | 'ghost' | 'danger'>('secondary');
  readonly size = input<'sm' | 'md'>('md');
  readonly loading = input(false);

  protected readonly cls = computed(() => {
    const base =
      'inline-flex items-center justify-center gap-1.5 rounded-lg font-medium transition-colors ' +
      'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue-500 ' +
      'disabled:opacity-50 disabled:pointer-events-none cursor-pointer';

    const sizes: Record<string, string> = {
      sm: 'h-7 px-2.5 text-xs',
      md: 'h-8 px-3 text-sm',
    };

    const variants: Record<string, string> = {
      primary: 'bg-blue-600 text-white hover:bg-blue-700',
      secondary: 'bg-slate-100 text-slate-700 hover:bg-slate-200',
      ghost: 'text-slate-500 hover:text-slate-900 hover:bg-slate-100',
      danger: 'bg-red-50 text-red-600 hover:bg-red-100',
    };

    return `${base} ${sizes[this.size()]} ${variants[this.variant()]}`;
  });
}
