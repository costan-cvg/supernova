/**
 * <centurisk-dashboard> — Portfolio overview with TIV breakdown and quality summary.
 */

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host { display: block; }
    .stats-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 1rem; margin-bottom: 1.5rem; }
    .stat-card { background: #fff; border-radius: 6px; padding: 1.25rem; box-shadow: 0 1px 2px rgba(0,0,0,0.05); }
    .stat-label { font-size: 0.75rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted, #718096); }
    .stat-value { font-size: 1.75rem; font-weight: 700; color: var(--color-primary, #1a365d); margin-top: 0.25rem; }
    .stat-value.money { font-variant-numeric: tabular-nums; }

    h3 { font-size: 1rem; font-weight: 600; color: var(--color-text, #2d3748); margin: 0 0 1rem; }
    .section { margin-bottom: 2rem; }
    .card { background: #fff; border-radius: 6px; padding: 1.25rem; box-shadow: 0 1px 2px rgba(0,0,0,0.05); }
    .tiv-controls { display: flex; gap: 0.75rem; margin-bottom: 1rem; }
    .tiv-controls select { padding: 0.375rem 0.75rem; border: 1px solid var(--color-border, #e2e8f0); border-radius: 4px; font-size: 0.875rem; font-family: inherit; }

    .bar-chart { display: flex; flex-direction: column; gap: 0.5rem; }
    .bar-row { display: flex; align-items: center; gap: 0.75rem; }
    .bar-label { width: 140px; font-size: 0.8125rem; color: var(--color-text, #2d3748); text-align: right; flex-shrink: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .bar-track { flex: 1; height: 24px; background: #edf2f7; border-radius: 4px; overflow: hidden; position: relative; }
    .bar-fill { height: 100%; background: var(--color-primary, #1a365d); border-radius: 4px; transition: width 0.3s ease; min-width: 2px; }
    .bar-value { width: 100px; font-size: 0.8125rem; color: var(--color-text-muted, #718096); font-variant-numeric: tabular-nums; }

    .type-pills { display: flex; gap: 0.5rem; flex-wrap: wrap; }
    .pill { padding: 0.375rem 0.75rem; background: #edf2f7; border-radius: 9999px; font-size: 0.8125rem; }
    .pill strong { font-weight: 600; }

    .loading { text-align: center; padding: 2rem; color: var(--color-text-muted, #718096); }
    .empty-state { text-align: center; padding: 3rem; color: var(--color-text-muted, #718096); background: #fff; border-radius: 6px; border: 1px dashed var(--color-border, #e2e8f0); }
    .empty-state p { margin-bottom: 1rem; }
    .btn-primary { padding: 0.5rem 1rem; background: var(--color-primary, #1a365d); color: #fff; border: none; border-radius: 4px; font-size: 0.875rem; cursor: pointer; }

    @media (max-width: 768px) { .stats-grid { grid-template-columns: repeat(2, 1fr); } }
</style>
<div id="content"><div class="loading">Loading dashboard...</div></div>
`;

class CenturiskDashboard extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._overview = null;
        this._tiv = null;
    }

    connectedCallback() { this._load(); }

    async _load() {
        try {
            const [ovResp, tivResp] = await Promise.all([
                fetch("/api/dashboard/overview"),
                fetch("/api/dashboard/tiv?group_by=city"),
            ]);
            this._overview = ovResp.ok ? await ovResp.json() : null;
            this._tiv = tivResp.ok ? await tivResp.json() : null;
        } catch (_) {}
        this._render();
    }

    async _loadTiv(groupBy) {
        try {
            const resp = await fetch("/api/dashboard/tiv?group_by=" + groupBy);
            this._tiv = resp.ok ? await resp.json() : null;
        } catch (_) {}
        this._renderTiv();
    }

    _render() {
        const content = this.shadowRoot.getElementById("content");

        if (!this._overview || this._overview.total_assets === 0) {
            content.innerHTML =
                '<div class="empty-state">' +
                '<p>No exposures in your portfolio yet.</p>' +
                '<button class="btn-primary" id="go-assets">View Exposures</button>' +
                '</div>';
            const btn = content.querySelector("#go-assets");
            if (btn) btn.addEventListener("click", () => {
                this.dispatchEvent(new CustomEvent("navigate", { detail: { page: "assets" }, bubbles: true, composed: true }));
            });
            return;
        }

        const o = this._overview;
        const fmtMoney = (n) => "$" + Math.round(n).toLocaleString();

        let html = '<div class="stats-grid">';
        html += '<div class="stat-card"><div class="stat-label">Total Exposures</div><div class="stat-value">' + o.total_assets + '</div></div>';
        html += '<div class="stat-card"><div class="stat-label">Total Insured Value</div><div class="stat-value money">' + fmtMoney(o.total_tiv) + '</div></div>';
        html += '<div class="stat-card"><div class="stat-label">Pending Approvals</div><div class="stat-value">' + o.pending_approvals + '</div></div>';
        html += '<div class="stat-card"><div class="stat-label">Asset Types</div><div class="type-pills">';
        for (const t of o.by_type) {
            html += '<span class="pill"><strong>' + t.count + '</strong> ' + this._esc(t.label) + '</span>';
        }
        html += '</div></div></div>';

        html += '<div class="section"><div class="card"><h3>TIV Accumulation</h3>';
        html += '<div class="tiv-controls"><select id="tiv-group">';
        html += '<option value="city">By City</option>';
        html += '<option value="state">By State</option>';
        html += '<option value="zip_code">By ZIP Code</option>';
        html += '<option value="construction_class">By Construction Class</option>';
        html += '<option value="occupancy">By Occupancy</option>';
        html += '<option value="asset_type">By Asset Type</option>';
        html += '</select></div>';
        html += '<div id="tiv-chart"></div>';
        html += '</div></div>';

        content.innerHTML = html;

        content.querySelector("#tiv-group").addEventListener("change", (e) => {
            this._loadTiv(e.target.value);
        });

        this._renderTiv();
    }

    _renderTiv() {
        const chart = this.shadowRoot.getElementById("tiv-chart");
        if (!chart || !this._tiv) return;

        const t = this._tiv;
        if (t.buckets.length === 0) {
            chart.innerHTML = '<div class="loading">No TIV data.</div>';
            return;
        }

        const maxTiv = Math.max(...t.buckets.map(b => b.total_tiv));
        const fmtMoney = (n) => "$" + Math.round(n).toLocaleString();

        chart.innerHTML = '<div class="bar-chart">' + t.buckets.map(b => {
            const pct = maxTiv > 0 ? (b.total_tiv / maxTiv * 100) : 0;
            return '<div class="bar-row">' +
                '<span class="bar-label" title="' + this._esc(b.label) + '">' + this._esc(b.label) + '</span>' +
                '<div class="bar-track"><div class="bar-fill" style="width:' + pct + '%"></div></div>' +
                '<span class="bar-value">' + fmtMoney(b.total_tiv) + ' (' + b.asset_count + ')</span>' +
                '</div>';
        }).join("") + '</div>';
    }

    _esc(str) { const d = document.createElement("div"); d.textContent = str || ""; return d.innerHTML; }
}

customElements.define("centurisk-dashboard", CenturiskDashboard);
