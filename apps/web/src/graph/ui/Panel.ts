export abstract class Panel {
  readonly el: HTMLElement;

  constructor(tag?: string) {
    this.el = document.createElement(tag ?? 'div');
    this.el.className = 'panel';
  }

  abstract mount(): void | Promise<void>;
  abstract unmount(): void;
  destroy?(): void;
}
