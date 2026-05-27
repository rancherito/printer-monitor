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
      gap: 5px;
      border: 1px solid transparent;
      border-radius: var(--radius-sm, 6px);
      font-weight: 500;
      font-family: inherit;
      cursor: pointer;
      transition: background-color 0.15s, color 0.15s, border-color 0.15s;
      white-space: nowrap;
      line-height: 1;

      &:disabled { opacity: 0.5; cursor: not-allowed; }
      &:focus-visible {
        outline: 2px solid var(--ring, rgba(212, 212, 216, 0.7));
        outline-offset: 2px;
      }

      &--sm { padding: 4px 10px;  font-size: 0.75rem; height: 28px; }
      &--md { padding: 6px 14px;  font-size: 0.8125rem; height: 32px; }

      &--primary   { background: var(--accent);   border-color: var(--accent); color: #fff; }
      &--secondary { background: var(--bg-hover);  border-color: var(--border); color: var(--text-primary); }
      &--danger    { background: var(--danger);    border-color: var(--danger);  color: #fff; }
      &--ghost     { background: transparent;      color: var(--text-muted); }
      &--outline   { background: transparent;      border-color: var(--border); color: var(--text-primary); }

      &--primary:not(:disabled):hover   { background: #2563eb; border-color: #2563eb; }
      &--secondary:not(:disabled):hover { background: #3f3f46; border-color: #3f3f46; }
      &--danger:not(:disabled):hover    { background: #dc2626; border-color: #dc2626; }
      &--ghost:not(:disabled):hover     { background: var(--bg-hover); color: var(--text-primary); }
      &--outline:not(:disabled):hover   { background: var(--bg-hover); }
    }
  `],
})
export class BtnComponent {
  readonly type = input<'button' | 'submit' | 'reset'>('button');
  readonly disabled = input(false);
  readonly variant = input<'primary' | 'secondary' | 'danger' | 'ghost' | 'outline'>('secondary');
  readonly size = input<'sm' | 'md'>('md');
  readonly clicked = output<void>();
}
