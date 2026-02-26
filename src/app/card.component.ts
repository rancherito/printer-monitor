import { Component, input, computed, ChangeDetectionStrategy } from '@angular/core';

/**
 * Contenedor tipo card reutilizable.
 * Las clases base se aplican a nivel host para evitar wrappers innecesarios.
 *
 * Usage:
 *   <app-card>...</app-card>
 *   <app-card padding="lg">...</app-card>
 *   <app-card padding="none" class="overflow-hidden">...</app-card>
 */
@Component({
  selector: 'app-card',
  template: `<ng-content></ng-content>`,
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '[class]': 'cls()' },
})
export class CardComponent {
  readonly padding = input<'none' | 'sm' | 'md' | 'lg'>('md');

  protected readonly cls = computed(() => {
    const p: Record<string, string> = {
      none: '',
      sm: 'p-3',
      md: 'p-4',
      lg: 'p-6',
    };
    return `block bg-white border border-slate-200 rounded-xl shadow-sm ${p[this.padding()]}`;
  });
}
