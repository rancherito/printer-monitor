import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

@Component({
  selector: 'cd-button',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <button
      [type]="type()"
      [disabled]="disabled()"
      [class]="'btn btn--' + variant() + ' btn--' + size()"
      (click)="clicked.emit()"
    >
      <ng-content />
    </button>
  `,
  styles: [`
    :host { display: inline-flex; }

    .btn {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      gap: 4px;
      border: none;
      border-radius: 4px;
      font-weight: 500;
      font-family: inherit;
      cursor: pointer;
      transition: background-color 0.15s, color 0.15s;
      white-space: nowrap;
      line-height: 1;

      &:disabled { opacity: 0.5; cursor: not-allowed; }

      &--sm { padding: 3px 8px;  font-size: 0.7rem; }
      &--md { padding: 5px 12px; font-size: 0.75rem; }

      &--primary   { background: var(--accent);  color: #fff; }
      &--secondary { background: var(--bg-hover); color: var(--text-primary); }
      &--danger    { background: var(--danger);   color: #fff; }
      &--ghost     { background: transparent;     color: var(--text-muted); }

      &--primary:not(:disabled):hover   { background: #2563eb; }
      &--secondary:not(:disabled):hover { background: #52525b; }
      &--danger:not(:disabled):hover    { background: #dc2626; }
      &--ghost:not(:disabled):hover     { background: var(--bg-hover); color: var(--text-primary); }
    }
  `],
})
export class BtnComponent {
  readonly type = input<'button' | 'submit' | 'reset'>('button');
  readonly disabled = input(false);
  readonly variant = input<'primary' | 'secondary' | 'danger' | 'ghost'>('secondary');
  readonly size = input<'sm' | 'md'>('md');
  readonly clicked = output<void>();
}
