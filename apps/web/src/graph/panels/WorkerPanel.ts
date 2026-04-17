import { Panel } from '../ui/Panel.js';
import { ApiClient } from '../ui/ApiClient.js';

interface JobTypeStat {
  jobType: string;
  pending: number;
  running: number;
  completed: number;
  failed: number;
  oldest: string | null;
  newest: string | null;
}

interface WorkerQueueResponse {
  totals: { pending: number; running: number; completed: number; failed: number };
  jobTypes: JobTypeStat[];
}

const REFRESH_INTERVAL_MS = 3_000;

function formatJobType(raw: string): string {
  return raw.replace(/[-_]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function relativeTime(iso: string | null): string {
  if (!iso) return '--';
  const ms = Date.now() - Date.parse(iso);
  if (ms < 0) return 'just now';
  const sec = Math.floor(ms / 1000);
  if (sec < 60) return `${sec}s ago`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  return `${Math.floor(hr / 24)}d ago`;
}

export class WorkerPanel extends Panel {
  private timer: ReturnType<typeof setInterval> | null = null;
  private tableBody: HTMLElement | null = null;
  private totalsEl: HTMLElement | null = null;
  private updatedEl: HTMLElement | null = null;

  constructor() {
    super();
    this.el.className = 'panel worker-panel';
  }

  mount(): Promise<void> {
    this.el.innerHTML = '';
    this.renderShell();
    this.timer = setInterval(() => void this.refresh(), REFRESH_INTERVAL_MS);
    return this.refresh();
  }

  unmount(): void {
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  private renderShell(): void {
    // Header
    const header = document.createElement('div');
    header.className = 'worker-header';

    const title = document.createElement('div');
    title.className = 'worker-title';
    title.textContent = 'Worker Queue';

    this.updatedEl = document.createElement('span');
    this.updatedEl.style.cssText = 'font-size:0.72rem;color:var(--muted);';

    header.appendChild(title);
    header.appendChild(this.updatedEl);

    // Totals bar
    this.totalsEl = document.createElement('div');
    this.totalsEl.className = 'worker-totals';

    // Table
    const table = document.createElement('table');
    table.className = 'worker-table';

    const thead = document.createElement('thead');
    thead.innerHTML = `<tr>
      <th>Job Type</th>
      <th class="num">Pending</th>
      <th class="num">Running</th>
      <th class="num">Completed</th>
      <th class="num">Failed</th>
      <th>Oldest Queued</th>
      <th>Latest Queued</th>
    </tr>`;

    this.tableBody = document.createElement('tbody');
    table.appendChild(thead);
    table.appendChild(this.tableBody);

    this.el.appendChild(header);
    this.el.appendChild(this.totalsEl);
    this.el.appendChild(table);

    // Inject scoped styles
    this.injectStyles();
  }

  private async refresh(): Promise<void> {
    const data = (await ApiClient.getWorkerQueue()) as WorkerQueueResponse | null;

    if (!data) {
      if (this.tableBody) this.tableBody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:var(--muted);padding:24px;">Failed to load worker queue</td></tr>';
      return;
    }

    // Update totals
    if (this.totalsEl) {
      const t = data.totals;
      this.totalsEl.innerHTML =
        `<div class="total-card pending"><div class="total-num">${t.pending}</div><div class="total-label">Pending</div></div>` +
        `<div class="total-card running"><div class="total-num">${t.running}</div><div class="total-label">Running</div></div>` +
        `<div class="total-card completed"><div class="total-num">${t.completed}</div><div class="total-label">Completed</div></div>` +
        `<div class="total-card failed"><div class="total-num">${t.failed}</div><div class="total-label">Failed</div></div>`;
    }

    // Update table
    if (this.tableBody) {
      if (data.jobTypes.length === 0) {
        this.tableBody.innerHTML = '<tr><td colspan="7" style="text-align:center;color:var(--muted);padding:24px;">No jobs in queue</td></tr>';
      } else {
        this.tableBody.innerHTML = data.jobTypes.map((j) => {
          const pendingClass = j.pending > 0 ? ' class="highlight-pending"' : '';
          const runningClass = j.running > 0 ? ' class="highlight-running"' : '';
          const failedClass = j.failed > 0 ? ' class="highlight-failed"' : '';
          return `<tr>
            <td class="job-type">${formatJobType(j.jobType)}</td>
            <td class="num"${pendingClass}>${j.pending}</td>
            <td class="num"${runningClass}>${j.running}</td>
            <td class="num">${j.completed.toLocaleString()}</td>
            <td class="num"${failedClass}>${j.failed}</td>
            <td class="time">${relativeTime(j.oldest)}</td>
            <td class="time">${relativeTime(j.newest)}</td>
          </tr>`;
        }).join('');
      }
    }

    if (this.updatedEl) {
      this.updatedEl.textContent = `Updated ${new Date().toLocaleTimeString()}`;
    }
  }

  private injectStyles(): void {
    if (document.getElementById('worker-panel-styles')) return;
    const style = document.createElement('style');
    style.id = 'worker-panel-styles';
    style.textContent = `
      .worker-panel {
        padding: 16px 20px;
        overflow-y: auto;
      }
      .worker-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        margin-bottom: 16px;
      }
      .worker-title {
        font-size: 1.1rem;
        font-weight: 600;
        color: var(--ink);
      }
      .worker-totals {
        display: grid;
        grid-template-columns: repeat(4, 1fr);
        gap: 12px;
        margin-bottom: 20px;
      }
      .total-card {
        background: var(--panel);
        border: 1px solid var(--panel-border);
        border-radius: 8px;
        padding: 14px 16px;
        text-align: center;
      }
      .total-card.pending { border-left: 3px solid #f0883e; }
      .total-card.running { border-left: 3px solid #58a6ff; }
      .total-card.completed { border-left: 3px solid #39d98a; }
      .total-card.failed { border-left: 3px solid #f85149; }
      .total-num {
        font-size: 1.8rem;
        font-weight: 700;
        color: var(--ink);
        line-height: 1.1;
      }
      .total-label {
        font-size: 0.72rem;
        color: var(--muted);
        text-transform: uppercase;
        letter-spacing: 0.04em;
        margin-top: 4px;
      }
      .worker-table {
        width: 100%;
        border-collapse: collapse;
        font-size: 0.82rem;
      }
      .worker-table th {
        text-align: left;
        font-size: 0.7rem;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--muted);
        padding: 8px 12px;
        border-bottom: 1px solid var(--panel-border);
      }
      .worker-table th.num,
      .worker-table td.num {
        text-align: right;
      }
      .worker-table td {
        padding: 10px 12px;
        border-bottom: 1px solid rgba(139, 148, 158, 0.08);
        color: var(--ink);
      }
      .worker-table tr:hover td {
        background: rgba(139, 148, 158, 0.05);
      }
      .worker-table .job-type {
        font-weight: 500;
      }
      .worker-table .time {
        color: var(--muted);
        font-size: 0.76rem;
      }
      .highlight-pending {
        color: #f0883e !important;
        font-weight: 600;
      }
      .highlight-running {
        color: #58a6ff !important;
        font-weight: 600;
      }
      .highlight-failed {
        color: #f85149 !important;
        font-weight: 600;
      }
    `;
    document.head.appendChild(style);
  }
}
