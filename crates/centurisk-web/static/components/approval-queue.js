/**
 * <centurisk-approval-queue> — Pool admin approval queue for pending mutations.
 */

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host { display: block; }
    h2 { font-size: 1.125rem; font-weight: 600; color: var(--color-text, #2d3748); margin-bottom: 1rem; }
    .stats { display: flex; gap: 1rem; margin-bottom: 1.5rem; }
    .stat-card { background: #fff; border-radius: 6px; padding: 1rem 1.5rem; box-shadow: 0 1px 2px rgba(0,0,0,0.05); text-align: center; }
    .stat-value { font-size: 1.5rem; font-weight: 700; color: var(--color-primary, #1a365d); }
    .stat-label { font-size: 0.75rem; color: var(--color-text-muted, #718096); text-transform: uppercase; }

    .card { background: #fff; border-radius: 6px; box-shadow: 0 1px 2px rgba(0,0,0,0.05); margin-bottom: 1rem; padding: 1rem 1.25rem; display: flex; align-items: center; gap: 1rem; }
    .card-body { flex: 1; }
    .card-title { font-weight: 600; font-size: 0.9375rem; color: var(--color-text, #2d3748); }
    .card-meta { font-size: 0.8125rem; color: var(--color-text-muted, #718096); margin-top: 0.25rem; }
    .card-diff { display: flex; gap: 0.5rem; align-items: center; margin-top: 0.375rem; font-size: 0.875rem; }
    .diff-old { color: #9b2c2c; text-decoration: line-through; }
    .diff-arrow { color: var(--color-text-muted, #718096); }
    .diff-new { color: #276749; font-weight: 500; }
    .card-actions { display: flex; gap: 0.5rem; flex-shrink: 0; }
    .btn-approve { padding: 0.375rem 0.75rem; background: #276749; color: #fff; border: none; border-radius: 4px; font-size: 0.8125rem; cursor: pointer; }
    .btn-reject { padding: 0.375rem 0.75rem; background: #fff; color: #9b2c2c; border: 1px solid #feb2b2; border-radius: 4px; font-size: 0.8125rem; cursor: pointer; }
    .btn-approve:hover { opacity: 0.9; }
    .btn-reject:hover { background: #fff5f5; }
    .val-badge { display: inline-block; padding: 0.0625rem 0.375rem; border-radius: 4px; font-size: 0.6875rem; font-weight: 600; background: #fed7e2; color: #c53030; margin-left: 0.375rem; }
    .empty { text-align: center; padding: 3rem; color: var(--color-text-muted, #718096); background: #fff; border-radius: 6px; border: 1px dashed var(--color-border, #e2e8f0); }
    .forbidden { text-align: center; padding: 3rem; color: var(--color-text-muted, #718096); }
</style>
<h2>Approval Queue</h2>
<div id="stats" class="stats"></div>
<div id="content"></div>
`;

class CenturiskApprovalQueue extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._pending = [];
    }

    connectedCallback() { this._load(); }

    async _load() {
        const content = this.shadowRoot.getElementById("content");
        try {
            const resp = await fetch("/api/approvals");
            if (resp.status === 403) {
                content.innerHTML = '<div class="forbidden">Only pool administrators can access the approval queue.</div>';
                return;
            }
            if (!resp.ok) throw new Error(resp.statusText);
            this._pending = await resp.json();
        } catch (e) {
            this._pending = [];
        }
        this._render();
    }

    _render() {
        const stats = this.shadowRoot.getElementById("stats");
        const content = this.shadowRoot.getElementById("content");
        const valCount = this._pending.filter(p => p.is_valuation_field).length;

        stats.innerHTML =
            '<div class="stat-card"><div class="stat-value">' + this._pending.length + '</div><div class="stat-label">Pending</div></div>' +
            '<div class="stat-card"><div class="stat-value">' + valCount + '</div><div class="stat-label">Valuation Changes</div></div>';

        if (this._pending.length === 0) {
            content.innerHTML = '<div class="empty">No pending changes to review.</div>';
            return;
        }

        content.innerHTML = this._pending.map(p => {
            const field = p.field_name.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase());
            const valBadge = p.is_valuation_field ? '<span class="val-badge">Valuation</span>' : '';

            let diff = '<div class="card-diff">';
            if (p.previous_value) {
                diff += '<span class="diff-old">' + this._esc(p.previous_value) + '</span>';
                diff += '<span class="diff-arrow">&rarr;</span>';
            }
            diff += '<span class="diff-new">' + this._esc(p.proposed_value) + '</span>';
            diff += '</div>';

            return '<div class="card" data-id="' + p.mutation_id + '">' +
                '<div class="card-body">' +
                '<div class="card-title">' + this._esc(p.asset_name) + ' &mdash; ' + this._esc(field) + valBadge + '</div>' +
                '<div class="card-meta">Submitted ' + this._esc(p.submitted_at.substring(0, 10)) + ' &middot; ' + this._esc(p.asset_type) + '</div>' +
                diff +
                '</div>' +
                '<div class="card-actions">' +
                '<button class="btn-approve" data-action="approve" data-id="' + p.mutation_id + '">Approve</button>' +
                '<button class="btn-reject" data-action="reject" data-id="' + p.mutation_id + '">Reject</button>' +
                '</div></div>';
        }).join("");

        content.querySelectorAll("button[data-action]").forEach(btn => {
            btn.addEventListener("click", () => this._act(btn.dataset.id, btn.dataset.action));
        });
    }

    async _act(mutationId, decision) {
        try {
            const resp = await fetch("/api/approvals/" + mutationId, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ decision }),
            });
            if (!resp.ok) throw new Error("Action failed");

            // Remove from list and re-render
            this._pending = this._pending.filter(p => p.mutation_id !== mutationId);
            this._render();
        } catch (e) {
            alert("Error: " + e.message);
        }
    }

    _esc(str) { const d = document.createElement("div"); d.textContent = str || ""; return d.innerHTML; }
}

customElements.define("centurisk-approval-queue", CenturiskApprovalQueue);
