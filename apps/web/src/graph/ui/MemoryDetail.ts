import {
  renderTypeBadge,
  renderAuthorityBadge,
  renderVerificationBadge,
  renderConflictIndicator,
} from './Badges.js';

function collapsibleSection(title: string, content: HTMLElement, startOpen = false): HTMLElement {
  const wrapper = document.createElement('div');
  wrapper.className = 'collapsible';

  const header = document.createElement('div');
  header.className = 'collapsible-header';
  header.textContent = title;
  header.setAttribute('aria-expanded', String(startOpen));

  const body = document.createElement('div');
  body.className = 'collapsible-body';
  body.style.display = startOpen ? '' : 'none';
  body.appendChild(content);

  header.addEventListener('click', () => {
    const open = body.style.display !== 'none';
    body.style.display = open ? 'none' : '';
    header.setAttribute('aria-expanded', String(!open));
  });

  wrapper.appendChild(header);
  wrapper.appendChild(body);
  return wrapper;
}

export function renderMemoryDetail(memory: Record<string, unknown>, container: HTMLElement): void {
  container.innerHTML = '';

  // Header row
  const header = document.createElement('div');
  header.className = 'memory-header';

  const title = document.createElement('div');
  title.className = 'memory-title';
  title.textContent = String(memory['title'] ?? '(untitled)');
  header.appendChild(title);

  const badges = document.createElement('div');
  badges.className = 'memory-badges';
  if (memory['type']) badges.appendChild(renderTypeBadge(String(memory['type'])));
  if (memory['authority_class']) badges.appendChild(renderAuthorityBadge(String(memory['authority_class'])));
  if (memory['verification_status']) badges.appendChild(renderVerificationBadge(String(memory['verification_status'])));
  const conflictCount = Number(memory['conflict_count'] ?? 0);
  if (conflictCount > 0) badges.appendChild(renderConflictIndicator(conflictCount));
  header.appendChild(badges);
  container.appendChild(header);

  // Summary
  if (memory['summary']) {
    const summary = document.createElement('p');
    summary.className = 'memory-summary';
    summary.textContent = String(memory['summary']);
    container.appendChild(summary);
  }

  // Provenance section
  const provenance = memory['provenance'];
  if (Array.isArray(provenance) && provenance.length > 0) {
    const list = document.createElement('ul');
    list.className = 'provenance-list';
    for (const item of provenance) {
      const li = document.createElement('li');
      li.textContent = typeof item === 'object' && item !== null
        ? String((item as Record<string, unknown>)['source_ref'] ?? JSON.stringify(item))
        : String(item);
      list.appendChild(li);
    }
    container.appendChild(collapsibleSection('Provenance', list, false));
  }

  // Claims section
  const claims = memory['claims'];
  if (Array.isArray(claims) && claims.length > 0) {
    const claimsEl = document.createElement('div');
    claimsEl.className = 'claims-list';
    for (const claim of claims as Record<string, unknown>[]) {
      const row = document.createElement('div');
      row.className = 'claim-row';
      const label = document.createElement('span');
      label.className = 'claim-label';
      label.textContent = String(claim['title'] ?? claim['id'] ?? '(claim)');
      row.appendChild(label);
      if (claim['type']) row.appendChild(renderTypeBadge(String(claim['type'])));
      if (claim['status']) row.appendChild(renderVerificationBadge(String(claim['status'])));
      claimsEl.appendChild(row);
    }
    container.appendChild(collapsibleSection('Claims', claimsEl, false));
  }

  // Supersession chain
  const supersededBy = memory['superseded_by'];
  if (supersededBy) {
    const chain = document.createElement('div');
    chain.className = 'supersession-chain';
    const label = document.createElement('span');
    label.className = 'supersession-label';
    label.textContent = 'Superseded by: ';
    const ref = document.createElement('span');
    ref.className = 'supersession-ref';
    ref.textContent = String(supersededBy);
    chain.appendChild(label);
    chain.appendChild(ref);
    container.appendChild(chain);
  }

  // Content section
  if (memory['content']) {
    const pre = document.createElement('pre');
    pre.className = 'memory-content';
    pre.textContent = String(memory['content']);
    container.appendChild(collapsibleSection('Content', pre, false));
  }
}
