/**
 * <centurisk-renewal-page> — Renewal workflow: view proposals, approve/flag.
 * Shows a list of renewals, and drill into proposals for a selected renewal.
 */

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host { display: block; }
    h2 { font-size: 1.125rem; font-weight: 600; color: var(--color-text, #2d3748); margin-bottom: 1rem; }
    .card { background: #fff; border-radius: 6px; padding: 1.25rem; box-shadow: 0 1px 2px rgba(0,0,0,0.05); margin-bottom: 1rem; }
    .renewal-item { display: flex; justify-content: space-between; align-items: center; cursor: pointer; }
    .renewal-item:hover { opacity: 0.8; }
    .renewal-name { font-weight: 600; font-size: 0.9375rem; }
    .renewal-stats { font-size: 0.8125rem; color: var(--color-text-muted, #718096); }
    .badge { display: inline-block; padding: 0.125rem 0.5rem; border-radius: 9999px; font-size: 0.75rem; font-weight: 500; }
    .badge-open { background: #fefcbf; color: #975a16; }
    .badge-completed { background: #c6f6d5; color: #276749; }

    .back-btn { background: none; border: 1px solid var(--color-border, #e2e8f0); border-radius: 4px; padding: 0.375rem 0.75rem; font-size: 0.875rem; cursor: pointer; margin-bottom: 1rem; }

    table { width: 100%; border-collapse: collapse; }
    th { text-align: left; padding: 0.5rem 0.75rem; font-size: 0.75rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted, #718096); border-bottom: 1px solid var(--color-border, #e2e8f0); }
    td { padding: 0.5rem 0.75rem; font-size: 0.8125rem; border-bottom: 1px solid var(--color-border, #e2e8f0); }
    .diff-old { color: #9b2c2c; text-decoration: line-through; }
    .diff-new { color: #276749; font-weight: 500; }
    .decision-approved { color: #276749; font-weight: 500; }
    .decision-flagged { color: #c53030; font-weight: 500; }

    .btn-sm { padding: 0.25rem 0.5rem; border-radius: 4px; font-size: 0.75rem; cursor: pointer; border: none; }
    .btn-approve { background: #276749; color: #fff; }
    .btn-flag { background: #fff; color: #c53030; border: 1px solid #feb2b2; }
    .btn-bulk { padding: 0.5rem 1rem; background: var(--color-primary, #1a365d); color: #fff; border: none; border-radius: 4px; font-size: 0.875rem; cursor: pointer; margin-top: 1rem; }
    .btn-bulk:disabled { opacity: 0.5; cursor: not-allowed; }

    .flag-card { border-left: 3px solid #fc8181; padding-left: 0.75rem; margin-bottom: 0.75rem; }
    .flag-note { font-size: 0.8125rem; color: var(--color-text, #2d3748); }
    .flag-meta { font-size: 0.75rem; color: var(--color-text-muted, #718096); }
    .btn-resolve { padding: 0.25rem 0.5rem; background: #276749; color: #fff; border: none; border-radius: 4px; font-size: 0.75rem; cursor: pointer; }

    .empty { text-align: center; padding: 2rem; color: var(--color-text-muted, #718096); }
</style>
<div id="content"><div class="empty">Loading renewals...</div></div>
`;

class CenturiskRenewalPage extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._renewals = [];
        this._selectedRenewal = null;
        this._proposals = [];
        this._flags = [];
    }

    connectedCallback() { this._loadRenewals(); }

    async _loadRenewals() {
        try {
            const resp = await fetch("/api/renewals");
            this._renewals = resp.ok ? await resp.json() : [];
        } catch (_) { this._renewals = []; }
        this._renderList();
    }

    _renderList() {
        const content = this.shadowRoot.getElementById("content");
        if (this._renewals.length === 0) {
            content.innerHTML = '<h2>Renewals</h2><div class="empty">No active renewals.</div>';
            return;
        }

        content.innerHTML = '<h2>Renewals</h2>' + this._renewals.map(r => {
            const badgeCls = r.status === "Completed" ? "badge-completed" : "badge-open";
            return '<div class="card renewal-item" data-id="' + r.renewal_id + '">' +
                '<div><div class="renewal-name">' + this._esc(r.name) + ' <span class="badge ' + badgeCls + '">' + r.status + '</span></div>' +
                '<div class="renewal-stats">' + r.total_proposals + ' proposals &middot; ' + r.approved + ' approved &middot; ' + r.flagged + ' flagged &middot; ' + r.pending + ' pending</div></div></div>';
        }).join("");

        content.querySelectorAll(".renewal-item").forEach(el => {
            el.addEventListener("click", () => this._selectRenewal(el.dataset.id));
        });
    }

    async _selectRenewal(renewalId) {
        this._selectedRenewal = renewalId;
        try {
            const [pResp, fResp] = await Promise.all([
                fetch("/api/renewals/" + renewalId + "/proposals"),
                fetch("/api/renewals/" + renewalId + "/flags"),
            ]);
            this._proposals = pResp.ok ? await pResp.json() : [];
            this._flags = fResp.ok ? await fResp.json() : [];
        } catch (_) {}
        this._renderDetail();
    }

    _renderDetail() {
        const content = this.shadowRoot.getElementById("content");
        const r = this._renewals.find(r => r.renewal_id === this._selectedRenewal);
        const openFlags = this._flags.filter(f => f.state === "Open").length;

        let html = '<button class="back-btn" id="back-btn">&larr; All Renewals</button>';
        html += '<h2>' + this._esc(r?.name || "Renewal") + '</h2>';

        // Flags section
        if (this._flags.length > 0) {
            html += '<div class="card"><h3 style="font-size:0.875rem;font-weight:600;margin:0 0 0.75rem;">Flags (' + openFlags + ' open)</h3>';
            for (const f of this._flags) {
                html += '<div class="flag-card"><div class="flag-note">' + this._esc(f.asset_name) + ': ' + this._esc(f.member_note) + '</div>';
                html += '<div class="flag-meta">' + this._esc(f.state) + ' &middot; ' + this._esc(f.created_at.substring(0, 10)) + '</div>';
                if (f.state === "Open") {
                    html += '<button class="btn-resolve" data-flag="' + f.flag_id + '">Resolve</button>';
                }
                html += '</div>';
            }
            html += '</div>';
        }

        // Proposals table
        html += '<div class="card"><table><thead><tr><th>Asset</th><th>Field</th><th>Current</th><th>Proposed</th><th>Decision</th><th></th></tr></thead><tbody>';
        for (const p of this._proposals) {
            const field = p.field_name.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase());
            const decCls = p.member_decision === "Approved" ? "decision-approved" : p.member_decision === "Flagged" ? "decision-flagged" : "";
            const actions = p.member_decision ? '' :
                '<button class="btn-sm btn-approve" data-pid="' + p.proposal_id + '" data-action="approve">Approve</button> ' +
                '<button class="btn-sm btn-flag" data-pid="' + p.proposal_id + '" data-action="flag">Flag</button>';

            html += '<tr><td>' + this._esc(p.asset_name) + '</td><td>' + this._esc(field) + '</td>';
            html += '<td class="diff-old">' + this._esc(p.current_value || "—") + '</td>';
            html += '<td class="diff-new">' + this._esc(p.proposed_value) + '</td>';
            html += '<td class="' + decCls + '">' + this._esc(p.member_decision || "Pending") + '</td>';
            html += '<td>' + actions + '</td></tr>';
        }
        html += '</tbody></table>';

        // Bulk approve button
        const undecided = this._proposals.filter(p => !p.member_decision).length;
        if (undecided > 0) {
            html += '<button class="btn-bulk" id="bulk-btn"' + (openFlags > 0 ? ' disabled title="Resolve all flags first"' : '') + '>Bulk Approve ' + undecided + ' Remaining</button>';
        }
        html += '</div>';

        content.innerHTML = html;

        // Wire events
        content.querySelector("#back-btn")?.addEventListener("click", () => { this._selectedRenewal = null; this._renderList(); });

        content.querySelectorAll("[data-action]").forEach(btn => {
            btn.addEventListener("click", () => this._decide(btn.dataset.pid, btn.dataset.action));
        });

        content.querySelectorAll("[data-flag]").forEach(btn => {
            btn.addEventListener("click", () => this._resolveFlag(btn.dataset.flag));
        });

        content.querySelector("#bulk-btn")?.addEventListener("click", () => this._bulkApprove());
    }

    async _decide(proposalId, action) {
        const note = action === "flag" ? prompt("Enter a note for discussion:") : null;
        if (action === "flag" && !note) return;

        await fetch("/api/renewals/" + this._selectedRenewal + "/proposals/" + proposalId + "/decide", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ decision: action, note }),
        });
        this._selectRenewal(this._selectedRenewal);
    }

    async _resolveFlag(flagId) {
        await fetch("/api/renewals/" + this._selectedRenewal + "/flags/" + flagId + "/resolve", { method: "POST", headers: { "Content-Type": "application/json" }, body: "{}" });
        this._selectRenewal(this._selectedRenewal);
    }

    async _bulkApprove() {
        const resp = await fetch("/api/renewals/" + this._selectedRenewal + "/bulk-approve", { method: "POST", headers: { "Content-Type": "application/json" }, body: "{}" });
        if (!resp.ok) { const err = await resp.json(); alert(err.error); return; }
        this._selectRenewal(this._selectedRenewal);
    }

    _esc(str) { const d = document.createElement("div"); d.textContent = str || ""; return d.innerHTML; }
}

customElements.define("centurisk-renewal-page", CenturiskRenewalPage);
