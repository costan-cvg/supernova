/**
 * <centurisk-asset-list> — Tabular list of assets with click-to-detail.
 * Fetches from GET /api/assets and renders a sortable table.
 */

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host { display: block; }
    .toolbar { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; }
    .toolbar h2 { font-size: 1.125rem; font-weight: 600; color: var(--color-text, #2d3748); }
    .btn-primary { padding: 0.5rem 1rem; background: var(--color-primary, #1a365d); color: #fff; border: none; border-radius: 4px; font-size: 0.875rem; font-weight: 500; cursor: pointer; }
    .btn-primary:hover { opacity: 0.9; }
    table { width: 100%; border-collapse: collapse; background: #fff; border-radius: 6px; overflow: hidden; box-shadow: 0 1px 2px rgba(0,0,0,0.05); }
    th { text-align: left; padding: 0.75rem 1rem; font-size: 0.75rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted, #718096); background: #f7fafc; border-bottom: 1px solid var(--color-border, #e2e8f0); }
    td { padding: 0.75rem 1rem; font-size: 0.875rem; border-bottom: 1px solid var(--color-border, #e2e8f0); }
    tr:hover td { background: #f7fafc; cursor: pointer; }
    .badge { display: inline-block; padding: 0.125rem 0.5rem; border-radius: 9999px; font-size: 0.75rem; font-weight: 500; }
    .badge-building { background: #ebf4ff; color: #2b6cb0; }
    .badge-contents { background: #fefcbf; color: #975a16; }
    .badge-vehicle { background: #e9d8fd; color: #6b46c1; }
    .badge-finearts { background: #fed7e2; color: #c53030; }
    .badge-active { background: #c6f6d5; color: #276749; }
    .badge-draft { background: #e2e8f0; color: #4a5568; }
    .badge-pending { background: #fefcbf; color: #975a16; }
    .badge-archived { background: #fed7d7; color: #9b2c2c; }
    .empty-state { text-align: center; padding: 3rem 1rem; color: var(--color-text-muted, #718096); background: #fff; border-radius: 6px; border: 1px dashed var(--color-border, #e2e8f0); }
    .empty-state p { margin-bottom: 1rem; }
    .money { font-variant-numeric: tabular-nums; }
    .filters { display: flex; gap: 0.75rem; margin-bottom: 1rem; }
    .filters input, .filters select { padding: 0.375rem 0.75rem; border: 1px solid var(--color-border, #e2e8f0); border-radius: 4px; font-size: 0.875rem; font-family: inherit; }
    .filters input { flex: 1; }
    .filters select { min-width: 130px; }
</style>
<div class="toolbar">
    <h2>Assets</h2>
    <button class="btn-primary" id="add-btn">+ Add Asset</button>
</div>
<div class="filters" id="filters">
    <input type="text" id="search" placeholder="Search assets..." />
    <select id="type-filter">
        <option value="">All Types</option>
        <option value="Building">Building</option>
        <option value="Contents">Contents</option>
        <option value="Vehicle">Vehicle</option>
        <option value="FineArts">Fine Arts</option>
    </select>
    <select id="lifecycle-filter">
        <option value="">All Statuses</option>
        <option value="Active">Active</option>
        <option value="Draft">Draft</option>
        <option value="PendingChange">Pending</option>
        <option value="Archived">Archived</option>
    </select>
</div>
<div id="content"></div>
`;

class CenturiskAssetList extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._assets = [];
    }

    connectedCallback() {
        this.shadowRoot.getElementById("add-btn").addEventListener("click", () => {
            this.dispatchEvent(new CustomEvent("navigate", {
                detail: { page: "asset-create" },
                bubbles: true,
                composed: true,
            }));
        });

        // Filter controls
        let debounce;
        const reload = () => { clearTimeout(debounce); debounce = setTimeout(() => this.load(), 200); };
        this.shadowRoot.getElementById("search").addEventListener("input", reload);
        this.shadowRoot.getElementById("type-filter").addEventListener("change", () => this.load());
        this.shadowRoot.getElementById("lifecycle-filter").addEventListener("change", () => this.load());

        this.load();
    }

    async load() {
        const params = new URLSearchParams();
        const search = this.shadowRoot.getElementById("search")?.value;
        const typeFilter = this.shadowRoot.getElementById("type-filter")?.value;
        const lifecycleFilter = this.shadowRoot.getElementById("lifecycle-filter")?.value;
        if (search) params.set("search", search);
        if (typeFilter) params.set("asset_type", typeFilter);
        if (lifecycleFilter) params.set("lifecycle", lifecycleFilter);

        try {
            const url = "/api/assets" + (params.toString() ? "?" + params.toString() : "");
            const resp = await fetch(url);
            if (!resp.ok) throw new Error(resp.statusText);
            this._assets = await resp.json();
        } catch (e) {
            this._assets = [];
        }
        this._render();
    }

    _render() {
        const content = this.shadowRoot.getElementById("content");

        if (this._assets.length === 0) {
            content.innerHTML =
                '<div class="empty-state">' +
                '<p>No assets yet.</p>' +
                '<button class="btn-primary" id="empty-add-btn">+ Add your first asset</button>' +
                '</div>';
            const btn = content.querySelector("#empty-add-btn");
            if (btn) {
                btn.addEventListener("click", () => {
                    this.dispatchEvent(new CustomEvent("navigate", {
                        detail: { page: "asset-create" },
                        bubbles: true,
                        composed: true,
                    }));
                });
            }
            return;
        }

        const rows = this._assets.map(a => {
            const name = a.fields?.building_name || a.fields?.name || (a.asset_type + " " + a.asset_id.substring(0, 8));
            const typeCls = "badge-" + a.asset_type.toLowerCase().replace(/\s/g, "");
            const lifeCls = "badge-" + a.lifecycle.toLowerCase().replace("pendingchange", "pending");
            const cost = a.fields?.replacement_cost || "\u2014";

            return '<tr data-id="' + this._esc(a.asset_id) + '">' +
                '<td>' + this._esc(name) + '</td>' +
                '<td><span class="badge ' + typeCls + '">' + this._esc(a.asset_type) + '</span></td>' +
                '<td>' + this._esc(a.fields?.address || "\u2014") + '</td>' +
                '<td><span class="badge ' + lifeCls + '">' + this._esc(a.lifecycle) + '</span></td>' +
                '<td class="money">' + this._esc(cost) + '</td>' +
                '</tr>';
        }).join("");

        content.innerHTML =
            '<table><thead><tr>' +
            '<th>Name</th><th>Type</th><th>Address</th><th>Status</th><th>Replacement Cost</th>' +
            '</tr></thead><tbody>' + rows + '</tbody></table>';

        content.querySelectorAll("tr[data-id]").forEach(row => {
            row.addEventListener("click", () => {
                this.dispatchEvent(new CustomEvent("navigate", {
                    detail: { page: "asset-detail", assetId: row.dataset.id },
                    bubbles: true,
                    composed: true,
                }));
            });
        });
    }

    _esc(str) {
        const div = document.createElement("div");
        div.textContent = str;
        return div.innerHTML;
    }
}

customElements.define("centurisk-asset-list", CenturiskAssetList);
