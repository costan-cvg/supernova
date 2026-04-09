/**
 * <centurisk-asset-card> — displays a single asset summary.
 *
 * Usage:
 *   const card = document.createElement("centurisk-asset-card");
 *   card.data = {
 *     asset_id: "...",
 *     asset_type: "Building",
 *     lifecycle: "Active",
 *     fields: {
 *       building_name: "Fire Station #7",
 *       address: "123 Main St",
 *       replacement_cost: "$1,500,000"
 *     }
 *   };
 */

const LIFECYCLE_COLORS = {
    Draft: { bg: "#edf2f7", text: "#4a5568", label: "Draft" },
    Active: { bg: "#c6f6d5", text: "#22543d", label: "Active" },
    PendingChange: { bg: "#fefcbf", text: "#744210", label: "Pending" },
    Archived: { bg: "#fed7d7", text: "#822727", label: "Archived" },
};

const ASSET_TYPE_LABELS = {
    Building: "Building",
    PropertyInTheOpen: "Property in the Open",
    MovableEquipment: "Movable Equipment",
    LicensedVehicle: "Licensed Vehicle",
    FineArts: "Fine Arts",
};

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host {
        display: block;
    }

    .card {
        background: #fff;
        border: 1px solid #e2e8f0;
        border-radius: 8px;
        padding: 1.25rem;
        transition: box-shadow 0.15s ease;
    }

    .card:hover {
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
    }

    .card-header {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 0.75rem;
        margin-bottom: 0.75rem;
    }

    .card-title {
        font-size: 1rem;
        font-weight: 600;
        color: #2d3748;
        margin: 0;
        line-height: 1.4;
    }

    .badge {
        display: inline-block;
        font-size: 0.6875rem;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.03em;
        padding: 0.2em 0.6em;
        border-radius: 4px;
        white-space: nowrap;
        line-height: 1.4;
    }

    .badge-type {
        background: #ebf4ff;
        color: #2b6cb0;
    }

    .badges {
        display: flex;
        gap: 0.375rem;
        flex-shrink: 0;
    }

    .card-body {
        display: flex;
        flex-direction: column;
        gap: 0.375rem;
    }

    .field {
        display: flex;
        align-items: baseline;
        gap: 0.5rem;
        font-size: 0.875rem;
        color: #4a5568;
    }

    .field-label {
        color: #718096;
        flex-shrink: 0;
    }

    .field-value {
        color: #2d3748;
        font-weight: 500;
    }

    .field-value.cost {
        font-variant-numeric: tabular-nums;
    }
</style>

<div class="card">
    <div class="card-header">
        <h3 class="card-title"></h3>
        <div class="badges">
            <span class="badge badge-type" data-ref="type-badge"></span>
            <span class="badge" data-ref="lifecycle-badge"></span>
        </div>
    </div>
    <div class="card-body">
        <div class="field" data-ref="address-row" hidden>
            <span class="field-label">Address</span>
            <span class="field-value" data-ref="address"></span>
        </div>
        <div class="field" data-ref="cost-row" hidden>
            <span class="field-label">Replacement</span>
            <span class="field-value cost" data-ref="cost"></span>
        </div>
    </div>
</div>
`;

class CenturiskAssetCard extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._data = null;
    }

    get data() {
        return this._data;
    }

    set data(value) {
        this._data = value;
        this._render();
    }

    _ref(name) {
        return this.shadowRoot.querySelector(`[data-ref="${name}"]`);
    }

    _render() {
        const d = this._data;
        if (!d) return;

        const fields = d.fields || {};

        // Title: building_name or fallback
        const name =
            fields.building_name ||
            `${ASSET_TYPE_LABELS[d.asset_type] || d.asset_type} ${(d.asset_id || "").substring(0, 8)}`;
        this.shadowRoot.querySelector(".card-title").textContent = name;

        // Asset type badge
        const typeBadge = this._ref("type-badge");
        typeBadge.textContent = ASSET_TYPE_LABELS[d.asset_type] || d.asset_type;

        // Lifecycle badge
        const lc = LIFECYCLE_COLORS[d.lifecycle] || LIFECYCLE_COLORS.Draft;
        const lcBadge = this._ref("lifecycle-badge");
        lcBadge.textContent = lc.label;
        lcBadge.style.background = lc.bg;
        lcBadge.style.color = lc.text;

        // Address
        const addressRow = this._ref("address-row");
        const addressVal = this._ref("address");
        if (fields.address) {
            addressRow.hidden = false;
            addressVal.textContent = fields.address;
        } else {
            addressRow.hidden = true;
        }

        // Replacement cost
        const costRow = this._ref("cost-row");
        const costVal = this._ref("cost");
        if (fields.replacement_cost) {
            costRow.hidden = false;
            costVal.textContent = fields.replacement_cost;
        } else {
            costRow.hidden = true;
        }
    }
}

customElements.define("centurisk-asset-card", CenturiskAssetCard);
