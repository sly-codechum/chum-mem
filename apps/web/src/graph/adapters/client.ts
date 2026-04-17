/**
 * Client entry point — bootstraps the Shell and registers all panels.
 * Bundled by esbuild and served as a static asset.
 */
import { Shell } from '../ui/Shell.js';
import { injectStyles } from '../ui/styles.js';
import { GraphPanel } from '../panels/GraphPanel.js';
import { ClaimExplorer } from '../panels/ClaimExplorer.js';
import { SearchWorkbench } from '../panels/SearchWorkbench.js';
import { CommunityPanel } from '../panels/CommunityPanel.js';
import { SessionTimeline } from '../panels/SessionTimeline.js';
import { WorkerPanel } from '../panels/WorkerPanel.js';

async function main() {
  injectStyles();

  const root = document.getElementById('app');
  if (!root) throw new Error('No #app element found');

  const shell = new Shell(root);

  // Register all panels
  const graphContainer = document.getElementById('graph-container')!;
  const graphPanel = new GraphPanel(graphContainer);
  shell.registerPanel('graph', graphPanel);
  shell.registerPanel('claims', new ClaimExplorer());
  shell.registerPanel('search', new SearchWorkbench());
  shell.registerPanel('communities', new CommunityPanel());
  shell.registerPanel('sessions', new SessionTimeline());
  shell.registerPanel('workers', new WorkerPanel());

  // Activate graph tab and mount it (graph is special-cased in Shell)
  shell.activateTab('graph');
  graphPanel.mount();

  // Load summary stats
  void shell.loadSummary();
}

main().catch(console.error);
