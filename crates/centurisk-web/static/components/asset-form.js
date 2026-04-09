/**
 * <centurisk-asset-form> — Form for creating a new asset.
 * Submits to POST /api/assets and emits "asset-created" on success.
 */

const ASSET_TYPES = [
    { value: "Building", label: "Building" },
    { value: "Contents", label: "Contents" },
    { value: "Vehicle", label: "Vehicle" },
    { value: "FineArts", label: "Fine Arts" },
];

const BUILDING_FIELDS = [
    { name: "building_name", label: "Building Name", type: "text", required: true },
    { name: "address", label: "Address", type: "text", required: true },
    { name: "city", label: "City", type: "text", required: false },
    { name: "state", label: "State", type: "text", required: false },
    { name: "zip_code", label: "ZIP Code", type: "text", required: false },
    { name: "year_built", label: "Year Built", type: "number", required: false },
    { name: "sq_footage", label: "Square Footage", type: "number", required: false },
    { name: "stories", label: "Stories", type: "number", required: false },
    { name: "construction_class", label: "Construction Class", type: "select",
      options: ["Frame", "Joisted Masonry", "Non-Combustible", "Masonry", "Fire Resistive"] },
    { name: "replacement_cost", label: "Replacement Cost ($)", type: "number", required: false },
];

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host {
        display: block;
    }

    .form-header {
        display: flex;
        align-items: center;
        gap: 1rem;
        margin-bottom: 1.5rem;
    }

    .back-btn {
        background: none;
        border: 1px solid var(--color-border, #e2e8f0);
        border-radius: 4px;
        padding: 0.375rem 0.75rem;
        font-size: 0.875rem;
        cursor: pointer;
        color: var(--color-text, #2d3748);
    }

    h2 {
        font-size: 1.125rem;
        font-weight: 600;
        color: var(--color-text, #2d3748);
    }

    .form-card {
        background: #fff;
        border-radius: 6px;
        padding: 1.5rem;
        box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
        max-width: 640px;
    }

    .form-group {
        margin-bottom: 1rem;
    }

    label {
        display: block;
        font-size: 0.8125rem;
        font-weight: 500;
        color: var(--color-text, #2d3748);
        margin-bottom: 0.25rem;
    }

    label .required {
        color: #e53e3e;
    }

    input, select {
        width: 100%;
        padding: 0.5rem 0.75rem;
        border: 1px solid var(--color-border, #e2e8f0);
        border-radius: 4px;
        font-size: 0.875rem;
        font-family: inherit;
        color: var(--color-text, #2d3748);
        background: #fff;
    }

    input:focus, select:focus {
        outline: none;
        border-color: var(--color-primary, #1a365d);
        box-shadow: 0 0 0 3px rgba(26, 54, 93, 0.1);
    }

    .form-row {
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: 1rem;
    }

    .form-actions {
        margin-top: 1.5rem;
        display: flex;
        gap: 0.75rem;
    }

    .btn-primary {
        padding: 0.5rem 1.25rem;
        background: var(--color-primary, #1a365d);
        color: #fff;
        border: none;
        border-radius: 4px;
        font-size: 0.875rem;
        font-weight: 500;
        cursor: pointer;
    }

    .btn-secondary {
        padding: 0.5rem 1.25rem;
        background: #fff;
        color: var(--color-text, #2d3748);
        border: 1px solid var(--color-border, #e2e8f0);
        border-radius: 4px;
        font-size: 0.875rem;
        cursor: pointer;
    }

    .error {
        color: #e53e3e;
        font-size: 0.8125rem;
        margin-top: 0.5rem;
    }
</style>

<div class="form-header">
    <button class="back-btn" id="back-btn">&larr; Back</button>
    <h2>Add Asset</h2>
</div>
<div class="form-card">
    <form id="asset-form">
        <div class="form-group">
            <label>Asset Type <span class="required">*</span></label>
            <select id="asset-type" required></select>
        </div>
        <div id="fields-container"></div>
        <div class="error" id="error" style="display:none;"></div>
        <div class="form-actions">
            <button type="submit" class="btn-primary">Create Asset</button>
            <button type="button" class="btn-secondary" id="cancel-btn">Cancel</button>
        </div>
    </form>
</div>
`;

class CenturiskAssetForm extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
    }

    connectedCallback() {
        const typeSelect = this.shadowRoot.getElementById("asset-type");
        ASSET_TYPES.forEach(t => {
            const opt = document.createElement("option");
            opt.value = t.value;
            opt.textContent = t.label;
            typeSelect.appendChild(opt);
        });

        typeSelect.addEventListener("change", () => this._renderFields());
        this._renderFields();

        this.shadowRoot.getElementById("asset-form").addEventListener("submit", (e) => {
            e.preventDefault();
            this._submit();
        });

        const goBack = () => {
            this.dispatchEvent(new CustomEvent("navigate", {
                detail: { page: "assets" },
                bubbles: true,
                composed: true,
            }));
        };
        this.shadowRoot.getElementById("back-btn").addEventListener("click", goBack);
        this.shadowRoot.getElementById("cancel-btn").addEventListener("click", goBack);
    }

    _renderFields() {
        const container = this.shadowRoot.getElementById("fields-container");
        // For now, show building fields for all types (other types get their specific fields in Inc 13)
        const fields = BUILDING_FIELDS;

        container.innerHTML = fields.map(f => {
            if (f.type === "select") {
                const opts = f.options.map(o => `<option value="${o}">${o}</option>`).join("");
                return `<div class="form-group">
                    <label>${f.label}</label>
                    <select name="${f.name}"><option value="">Select...</option>${opts}</select>
                </div>`;
            }
            const req = f.required ? '<span class="required">*</span>' : "";
            const reqAttr = f.required ? "required" : "";
            return `<div class="form-group">
                <label>${f.label} ${req}</label>
                <input type="${f.type}" name="${f.name}" ${reqAttr} />
            </div>`;
        }).join("");
    }

    async _submit() {
        const form = this.shadowRoot.getElementById("asset-form");
        const errorEl = this.shadowRoot.getElementById("error");
        errorEl.style.display = "none";

        const data = new FormData(form);
        const assetType = this.shadowRoot.getElementById("asset-type").value;

        const fields = {};
        for (const [key, val] of data.entries()) {
            if (key !== "asset-type" && val.trim()) {
                fields[key] = val.trim();
            }
        }

        try {
            const resp = await fetch("/api/assets", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ asset_type: assetType, fields }),
            });

            if (!resp.ok) {
                const err = await resp.json().catch(() => ({ error: resp.statusText }));
                throw new Error(err.error || "Failed to create asset");
            }

            this.dispatchEvent(new CustomEvent("navigate", {
                detail: { page: "assets" },
                bubbles: true,
                composed: true,
            }));
        } catch (e) {
            errorEl.textContent = e.message;
            errorEl.style.display = "block";
        }
    }
}

customElements.define("centurisk-asset-form", CenturiskAssetForm);
