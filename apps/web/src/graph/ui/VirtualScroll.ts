const BUFFER = 3; // extra items rendered above and below visible window

export class VirtualScroll {
  private outer: HTMLElement;
  private inner: HTMLElement;
  private spacer: HTMLElement;
  private count = 0;
  private visibleStart = 0;
  private visibleEnd = 0;
  private renderedItems: HTMLElement[] = [];

  constructor(
    container: HTMLElement,
    private readonly itemHeight: number,
    private readonly renderItem: (index: number) => HTMLElement,
  ) {
    this.outer = container;
    this.outer.style.overflowY = 'auto';
    this.outer.style.position = 'relative';

    // Full-height spacer to make the scrollbar accurate
    this.spacer = document.createElement('div');
    this.spacer.style.pointerEvents = 'none';

    // Positioned layer for rendered items
    this.inner = document.createElement('div');
    this.inner.style.position = 'absolute';
    this.inner.style.top = '0';
    this.inner.style.left = '0';
    this.inner.style.right = '0';

    this.outer.appendChild(this.spacer);
    this.outer.appendChild(this.inner);

    this.outer.addEventListener('scroll', () => this.refresh(), { passive: true });
  }

  setItemCount(count: number): void {
    this.count = count;
    this.spacer.style.height = `${this.count * this.itemHeight}px`;
    this.refresh();
  }

  refresh(): void {
    const scrollTop = this.outer.scrollTop;
    const viewHeight = this.outer.clientHeight;

    const start = Math.max(0, Math.floor(scrollTop / this.itemHeight) - BUFFER);
    const end = Math.min(this.count, Math.ceil((scrollTop + viewHeight) / this.itemHeight) + BUFFER);

    if (start === this.visibleStart && end === this.visibleEnd) return;

    this.visibleStart = start;
    this.visibleEnd = end;
    this.inner.innerHTML = '';
    this.renderedItems = [];

    for (let i = start; i < end; i++) {
      const el = this.renderItem(i);
      el.style.position = 'absolute';
      el.style.top = `${i * this.itemHeight}px`;
      el.style.left = '0';
      el.style.right = '0';
      el.style.height = `${this.itemHeight}px`;
      this.inner.appendChild(el);
      this.renderedItems.push(el);
    }
  }
}
