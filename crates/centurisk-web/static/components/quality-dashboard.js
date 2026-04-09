/**
 * <centurisk-quality-dashboard> — Pool-level quality overview.
 * Shows worst-first asset quality scores with completeness/accuracy breakdowns.
 */

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host { display: block; }
    h2 { font-size: 1.125rem; font-weight: 600; color: var(--color-text, #2d3748); margin-bottom: 1rem; }
    .stats { display: flex; gap: 1rem; margin-bottom: 1.5rem; }
    .stat-card { background: #fff; border-radius: 6px; padding: 1.25rem; box-shadow: 0 1px 2px rgba(0,0,0,0.05); text-align: center; flex: 1; }
    .stat-value { font-size: 2rem; font-weight: 700; }
    .stat-label { font-size: 0.75rem; color: var(--color-text-muted, #718096); text-transform: uppercase; margin-top: 0.25rem; }
    .score-green { color: #276749; }
    .score-yellow { color: #975a16; }
    .score-red { color: #9b2c2c; }

    .card { background: #fff; border-radius: 6px; padding: 1.25rem; box-shadow: 0 1px 2px rgba(0,0,0,0.05); }
    table { width: 100%; border-collapse: collapse; }
    th { text-align: left; padding: 0.5rem 0.75rem; font-size: 0.75rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted, #718096); border-bottom: 1px solid var(--color-border, #e2e8f0); }
    td { padding: 0.5rem 0.75rem; font-size: 0.8125rem; border-bottom: 1px solid var(--color-border, #e2e8f0); }
    tr:hover td { background: #f7fafc; cursor: pointer; }

    .score-bar { display: inline-block; width: 60px; height: 8px; background: #edf2f7; border-radius: 4px; overflow: hidden; vertical-align: middle; margin-right: 0.5rem; }
    .score-fill { height: 100%; border-radius: 4px; }
    .score-fill.green { background: #48bb78; }
    .score-fill.yellow { background: #ecc94b; }
    .score-fill.red { background: #fc8181; }

    .badge { display: inline-block; padding: 0.0625rem 0.375rem; border-radius: 4px; font-size: 0.6875rem; font-weight: 600; }
    .badge-building { background: #ebf4ff; color: #2b6cb0; }
    .badge-vehicle, .badge-licensedvehicle { background: #e9d8fd; color: #6b46c1; }
    .badge-propertyintheopen { background: #fefcbf; color: #975a16; }
    .badge-finearts { background: #fed7e2; color: #c53030; }

    .gap-tag { display: inline-block; padding: 0.0625rem 0.375rem; border-radius: 4px; font-size: 0.625rem; background: #fed7d7; color: #9b2c2c; margin: 0.0625rem; }

    .empty { text-align: center; padding: 3rem; color: var(--color-text-muted, #718096); }
</style>
<h2>Exposure Quality</h2>
<div id="stats" class="stats"></div>
<div id="content"><div class="empty">Loading quality data...</div></div>
`;

class CenturiskQualityDashboard extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
    }

    connectedCallback() { this._load(); }

    async _load() {
        try {
            const resp = await fetch("/api/quality/summary");
            if (!resp.ok) throw new Error(resp.statusText);
            this._data = await resp.json();
        } catch (_) {
            this._data = [];
        }
        this._render();
    }

    _render() {
        const stats = this.shadowRoot.getElementById("stats");
        const content = this.shadowRoot.getElementById("content");
        const data = this._data;

        if (!data || data.length === 0) {
            stats.innerHTML = "";
            content.innerHTML = '<div class="empty">No quality data available.</div>';
            return;
        }

        // Compute aggregates
        const avgComp = data.reduce((s, a) => s + a.completeness, 0) / data.length;
        const avgAcc = data.reduce((s, a) => s + a.accuracy, 0) / data.length;
        const avgComposite = data.reduce((s, a) => s + a.composite, 0) / data.length;
        const gapCount = data.filter(a => a.missing_required.length > 0).length;

        const color = (s) => s >= 0.8 ? "score-green" : s >= 0.5 ? "score-yellow" : "score-red";
        const pct = (s) => Math.round(s * 100) + "%";

        stats.innerHTML =
            '<div class="stat-card"><div class="stat-value ' + color(avgComposite) + '">' + pct(avgComposite) + '</div><div class="stat-label">Overall Quality</div></div>' +
            '<div class="stat-card"><div class="stat-value ' + color(avgComp) + '">' + pct(avgComp) + '</div><div class="stat-label">Avg Completeness</div></div>' +
            '<div class="stat-card"><div class="stat-value ' + color(avgAcc) + '">' + pct(avgAcc) + '</div><div class="stat-label">Avg Accuracy</div></div>' +
            '<div class="stat-card"><div class="stat-value ' + (gapCount > 0 ? "score-red" : "score-green") + '">' + gapCount + '</div><div class="stat-label">Exposures with Gaps</div></div>';

        // Table sorted worst-first
        const rows = data.map(a => {
            const compColor = a.completeness >= 0.8 ? "green" : a.completeness >= 0.5 ? "yellow" : "red";
            const accColor = a.accuracy >= 0.8 ? "green" : a.accuracy >= 0.5 ? "yellow" : "red";
            const typeCls = "badge-" + a.asset_type.toLowerCase();
            const gaps = a.missing_required.map(f =>
                '<span class="gap-tag">' + this._esc(f.replace(/_/g, " ")) + '</span>'
            ).join("");

            return '<tr data-id="' + a.asset_id + '">' +
                '<td>' + this._esc(a.name) + '</td>' +
                '<td><span class="badge ' + typeCls + '">' + this._esc(a.asset_type) + '</span></td>' +
                '<td><span class="score-bar"><span class="score-fill ' + compColor + '" style="width:' + Math.round(a.completeness * 100) + '%"></span></span>' + pct(a.completeness) + '</td>' +
                '<td><span class="score-bar"><span class="score-fill ' + accColor + '" style="width:' + Math.round(a.accuracy * 100) + '%"></span></span>' + pct(a.accuracy) + '</td>' +
                '<td>' + (gaps || '<span style="color:#276749;font-size:0.75rem;">\u2713 Complete</span>') + '</td>' +
                '</tr>';
        }).join("");

        content.innerHTML = '<div class="card"><table><thead><tr>' +
            '<th>Exposure</th><th>Type</th><th>Completeness</th><th>Accuracy</th><th>Missing Required</th>' +
            '</tr></thead><tbody>' + rows + '</tbody></table></div>';

        // Click to navigate to asset detail
        content.querySelectorAll("tr[data-id]").forEach(row => {
            row.addEventListener("click", () => {
                this.dispatchEvent(new CustomEvent("navigate", {
                    detail: { page: "asset-detail", assetId: row.dataset.id },
                    bubbles: true, composed: true,
                }));
            });
        });
    }

    _esc(str) { const d = document.createElement("div"); d.textContent = str || ""; return d.innerHTML; }
}

customElements.define("centurisk-quality-dashboard", CenturiskQualityDashboard);
