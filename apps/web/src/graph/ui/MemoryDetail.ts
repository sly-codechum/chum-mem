import {
  renderTypeBadge,
  renderAuthorityBadge,
  renderVerificationBadge,
  renderConflictIndicator,
} from './Badges.js';
import { ApiClient } from './ApiClient.js';

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

  // Governance actions
  const claimId = memory['claim_id'] ?? memory['id'];
  if (claimId) {
    const govSection = document.createElement('div');
    govSection.className = 'governance-actions';

    const currentState = String(memory['governance_state'] ?? 'active');
    const stateLabel = document.createElement('span');
    stateLabel.className = 'governance-state-label';
    stateLabel.textContent = `State: ${currentState}`;
    govSection.appendChild(stateLabel);

    const reasonInput = document.createElement('input');
    reasonInput.type = 'text';
    reasonInput.placeholder = 'Reason (optional)';
    reasonInput.className = 'governance-reason';
    govSection.appendChild(reasonInput);

    const btnRow = document.createElement('div');
    btnRow.className = 'governance-btn-row';

    const transitions: [string, string, string][] = [
      ['active', 'Reactivate', '#39d98a'],
      ['pinned', 'Pin', '#58a6ff'],
      ['archived', 'Archive', '#8b949e'],
      ['rejected', 'Reject', '#ff6b6b'],
    ];

    for (const [state, label, color] of transitions) {
      if (state === currentState) continue;
      const btn = document.createElement('button');
      btn.className = 'governance-btn';
      btn.textContent = label;
      btn.style.borderColor = color;
      btn.style.color = color;
      btn.addEventListener('click', async () => {
        btn.disabled = true;
        const reason = reasonInput.value.trim() || undefined;
        const result = await ApiClient.governClaim(String(claimId), state, reason);
        if (result) {
          stateLabel.textContent = `State: ${state}`;
          memory['governance_state'] = state;
          btnRow.querySelectorAll('button').forEach((b) => b.remove());
          for (const [s2, l2, c2] of transitions) {
            if (s2 === state) continue;
            const b2 = document.createElement('button');
            b2.className = 'governance-btn';
            b2.textContent = l2;
            b2.style.borderColor = c2;
            b2.style.color = c2;
            b2.addEventListener('click', () => {
              renderMemoryDetail(memory, container);
            });
            btnRow.appendChild(b2);
          }
        }
        btn.disabled = false;
      });
      btnRow.appendChild(btn);
    }

    govSection.appendChild(btnRow);
    container.appendChild(collapsibleSection('Governance', govSection, false));
  }

  // Content section
  if (memory['content']) {
    const pre = document.createElement('pre');
    pre.className = 'memory-content';
    pre.textContent = String(memory['content']);
    container.appendChild(collapsibleSection('Content', pre, false));
  }
}
