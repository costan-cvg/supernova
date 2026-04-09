/**
 * <centurisk-asset-detail> — Full asset detail with fields, edit, history, and temporal resolution.
 * Set .assetId property to load.
 */

const FIELD_ORDER = [
    "building_name", "address", "city", "state", "zip_code",
    "construction_class", "occupancy", "year_built",
    "sq_footage", "stories", "roof_type", "sprinkler",
    "replacement_cost", "contents_value"
];

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host { display: block; }
    .header { display: flex; align-items: center; gap: 1rem; margin-bottom: 1rem; flex-wrap: wrap; }
    .back-btn { background: none; border: 1px solid var(--color-border, #e2e8f0); border-radius: 4px; padding: 0.375rem 0.75rem; font-size: 0.875rem; cursor: pointer; color: var(--color-text, #2d3748); }
    h2 { font-size: 1.25rem; font-weight: 600; color: var(--color-text, #2d3748); margin: 0; }
    .badge { display: inline-block; padding: 0.125rem 0.5rem; border-radius: 9999px; font-size: 0.75rem; font-weight: 500; margin-left: 0.5rem; }
    .badge-building { background: #ebf4ff; color: #2b6cb0; }
    .badge-contents { background: #fefcbf; color: #975a16; }
    .badge-vehicle { background: #e9d8fd; color: #6b46c1; }
    .badge-finearts { background: #fed7e2; color: #c53030; }
    .badge-active { background: #c6f6d5; color: #276749; }
    .badge-draft { background: #e2e8f0; color: #4a5568; }

    .toolbar { display: flex; align-items: center; gap: 0.75rem; margin-bottom: 1rem; flex-wrap: wrap; }
    .tabs { display: flex; gap: 0; border-bottom: 1px solid var(--color-border, #e2e8f0); }
    .tab { padding: 0.625rem 1.25rem; font-size: 0.875rem; font-weight: 500; cursor: pointer; border: none; background: none; color: var(--color-text-muted, #718096); border-bottom: 2px solid transparent; }
    .tab:hover { color: var(--color-text, #2d3748); }
    .tab.active { color: var(--color-primary, #1a365d); border-bottom-color: var(--color-primary, #1a365d); }
    .as-of-bar { display: flex; align-items: center; gap: 0.5rem; flex: 1; justify-content: flex-end; }
    .as-of-bar label { font-size: 0.8125rem; color: var(--color-text-muted, #718096); }
    .as-of-bar input[type="date"] { padding: 0.25rem 0.5rem; border: 1px solid var(--color-border, #e2e8f0); border-radius: 4px; font-size: 0.8125rem; font-family: inherit; }
    .as-of-bar .clear-btn { font-size: 0.75rem; color: var(--color-primary, #1a365d); cursor: pointer; border: none; background: none; text-decoration: underline; }
    .as-of-notice { background: #fefcbf; color: #975a16; padding: 0.5rem 1rem; border-radius: 4px; font-size: 0.8125rem; margin-bottom: 1rem; }

    .card { background: #fff; border-radius: 6px; padding: 1.5rem; box-shadow: 0 1px 2px rgba(0,0,0,0.05); }
    .field-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }
    .field-item { }
    .field-label { font-size: 0.75rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted, #718096); margin-bottom: 0.25rem; }
    .field-value { font-size: 0.9375rem; color: var(--color-text, #2d3748); }
    .field-value.money { font-variant-numeric: tabular-nums; font-weight: 600; }

    .btn-edit { padding: 0.375rem 0.75rem; background: var(--color-primary, #1a365d); color: #fff; border: none; border-radius: 4px; font-size: 0.8125rem; cursor: pointer; }
    .btn-save { padding: 0.375rem 0.75rem; background: #276749; color: #fff; border: none; border-radius: 4px; font-size: 0.8125rem; cursor: pointer; }
    .btn-cancel { padding: 0.375rem 0.75rem; background: none; color: var(--color-text, #2d3748); border: 1px solid var(--color-border, #e2e8f0); border-radius: 4px; font-size: 0.8125rem; cursor: pointer; }

    .edit-input { width: 100%; padding: 0.375rem 0.5rem; border: 1px solid var(--color-border, #e2e8f0); border-radius: 4px; font-size: 0.875rem; font-family: inherit; }
    .edit-input:focus { outline: none; border-color: var(--color-primary, #1a365d); box-shadow: 0 0 0 2px rgba(26,54,93,0.1); }

    table { width: 100%; border-collapse: collapse; }
    th { text-align: left; padding: 0.5rem 0.75rem; font-size: 0.75rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted, #718096); border-bottom: 1px solid var(--color-border, #e2e8f0); }
    td { padding: 0.5rem 0.75rem; font-size: 0.8125rem; border-bottom: 1px solid var(--color-border, #e2e8f0); color: var(--color-text, #2d3748); }
    .state-approved { color: #276749; }
    .state-pending { color: #975a16; }
    .state-rejected { color: #9b2c2c; }
    .empty { text-align: center; padding: 2rem; color: var(--color-text-muted, #718096); }
    .loading { text-align: center; padding: 2rem; color: var(--color-text-muted, #718096); }
    .success { color: #276749; font-size: 0.8125rem; margin-top: 0.5rem; }

    .quality-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; margin-bottom: 1.5rem; }
    .quality-card { background: #fff; border-radius: 6px; padding: 1.25rem; box-shadow: 0 1px 2px rgba(0,0,0,0.05); text-align: center; }
    .quality-card h3 { font-size: 0.75rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-muted, #718096); margin: 0 0 0.5rem; }
    .score-value { font-size: 2rem; font-weight: 700; }
    .score-green { color: #276749; }
    .score-yellow { color: #975a16; }
    .score-red { color: #9b2c2c; }
    .score-label { font-size: 0.75rem; color: var(--color-text-muted, #718096); margin-top: 0.25rem; }
    .gap-list { margin-top: 1rem; }
    .gap-item { display: flex; align-items: center; gap: 0.5rem; padding: 0.375rem 0; font-size: 0.8125rem; border-bottom: 1px solid var(--color-border, #e2e8f0); }
    .gap-icon { color: #e53e3e; font-weight: 700; }
    .gap-warn { color: #d69e2e; font-weight: 700; }
</style>
<div class="header">
    <button class="back-btn" id="back-btn">&larr; Back</button>
    <h2 id="title">Loading...</h2>
</div>
<div class="toolbar">
    <div class="tabs">
        <button class="tab active" data-tab="fields">Fields</button>
        <button class="tab" data-tab="quality">Quality</button>
        <button class="tab" data-tab="history">History</button>
    </div>
    <div class="as-of-bar">
        <label for="as-of-date">View as of:</label>
        <input type="date" id="as-of-date" />
        <button class="clear-btn" id="clear-as-of" style="display:none;">Current</button>
    </div>
</div>
<div id="as-of-notice" style="display:none;"></div>
<div id="content"><div class="loading">Loading asset...</div></div>
`;

class CenturiskAssetDetail extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._asset = null;
        this._mutations = [];
        this._quality = null;
        this._activeTab = "fields";
        this._editing = false;
        this._asOfDate = null;
    }

    connectedCallback() {
        this.shadowRoot.getElementById("back-btn").addEventListener("click", () => {
            this.dispatchEvent(new CustomEvent("navigate", { detail: { page: "assets" }, bubbles: true, composed: true }));
        });
        this.shadowRoot.querySelectorAll(".tab").forEach(tab => {
            tab.addEventListener("click", () => {
                this._activeTab = tab.dataset.tab;
                this.shadowRoot.querySelectorAll(".tab").forEach(t => t.classList.toggle("active", t.dataset.tab === this._activeTab));
                this._renderContent();
            });
        });
        this.shadowRoot.getElementById("as-of-date").addEventListener("change", (e) => {
            this._asOfDate = e.target.value || null;
            this.shadowRoot.getElementById("clear-as-of").style.display = this._asOfDate ? "inline" : "none";
            this._editing = false;
            this._load();
        });
        this.shadowRoot.getElementById("clear-as-of").addEventListener("click", () => {
            this._asOfDate = null;
            this.shadowRoot.getElementById("as-of-date").value = "";
            this.shadowRoot.getElementById("clear-as-of").style.display = "none";
            this._load();
        });
        if (this._assetId) this._load();
    }

    set assetId(id) { this._assetId = id; if (this.isConnected) this._load(); }

    async _load() {
        try {
            let url = "/api/assets/" + this._assetId;
            if (this._asOfDate) url += "?as_of=" + this._asOfDate;

            const [assetResp, mutResp, qualResp] = await Promise.all([
                fetch(url),
                fetch("/api/assets/" + this._assetId + "/mutations"),
                fetch("/api/assets/" + this._assetId + "/quality"),
            ]);
            if (!assetResp.ok) throw new Error("Asset not found");
            this._asset = await assetResp.json();
            this._mutations = mutResp.ok ? await mutResp.json() : [];
            this._quality = qualResp.ok ? await qualResp.json() : null;
        } catch (e) {
            this._asset = null;
            this._mutations = [];
        }
        this._renderHeader();
        this._renderAsOfNotice();
        this._renderContent();
    }

    _renderHeader() {
        const title = this.shadowRoot.getElementById("title");
        if (!this._asset) { title.textContent = "Asset not found"; return; }
        const a = this._asset;
        const name = a.fields.building_name || a.asset_type + " " + a.asset_id.substring(0, 8);
        const typeCls = "badge-" + a.asset_type.toLowerCase().replace(/\s/g, "");
        const lifeCls = "badge-" + a.lifecycle.toLowerCase().replace("pendingchange", "pending");
        title.innerHTML = this._esc(name) +
            ' <span class="badge ' + typeCls + '">' + this._esc(a.asset_type) + '</span>' +
            ' <span class="badge ' + lifeCls + '">' + this._esc(a.lifecycle) + '</span>';
    }

    _renderAsOfNotice() {
        const notice = this.shadowRoot.getElementById("as-of-notice");
        if (this._asOfDate) {
            notice.style.display = "block";
            notice.className = "as-of-notice";
            notice.textContent = "Showing asset state as of " + this._asOfDate + ". Some fields may not have values at this date.";
        } else {
            notice.style.display = "none";
        }
    }

    _renderContent() {
        const content = this.shadowRoot.getElementById("content");
        if (!this._asset) { content.innerHTML = '<div class="empty">Asset not found.</div>'; return; }
        if (this._activeTab === "fields") this._renderFields(content);
        else if (this._activeTab === "quality") this._renderQuality(content);
        else this._renderHistory(content);
    }

    _renderFields(content) {
        const a = this._asset;
        const entries = [];
        for (const key of FIELD_ORDER) { if (a.fields[key]) entries.push([key, a.fields[key]]); }
        for (const [key, val] of Object.entries(a.fields)) { if (!FIELD_ORDER.includes(key)) entries.push([key, val]); }

        if (entries.length === 0) {
            content.innerHTML = '<div class="card"><div class="empty">No field data' + (this._asOfDate ? ' at this date' : '') + '.</div></div>';
            return;
        }

        if (this._editing) {
            this._renderEditForm(content, entries);
            return;
        }

        const user = JSON.parse(localStorage.getItem("centurisk_user") || "{}");
        const canWrite = !["MemberReadOnly", "PoolReadOnly", "CentuRiskAuditor"].includes(user.category);
        const editBtn = (this._asOfDate || !canWrite) ? '' : '<button class="btn-edit" id="edit-btn">Edit Fields</button>';
        const readOnlyNotice = canWrite ? '' : '<div style="background:#ebf4ff;color:#2b6cb0;padding:0.5rem 1rem;border-radius:4px;font-size:0.8125rem;margin-bottom:1rem;">You have read-only access. Editing is not available.</div>';
        const items = entries.map(([key, val]) => {
            const label = key.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase());
            const cls = (key === "replacement_cost" || key === "contents_value") ? ' money' : '';
            return '<div class="field-item"><div class="field-label">' + this._esc(label) +
                '</div><div class="field-value' + cls + '">' + this._esc(val) + '</div></div>';
        }).join("");

        content.innerHTML = readOnlyNotice + '<div class="card">' + editBtn + '<div class="field-grid">' + items + '</div></div>';

        const eb = content.querySelector("#edit-btn");
        if (eb) eb.addEventListener("click", () => { this._editing = true; this._renderContent(); });
    }

    _renderEditForm(content, entries) {
        const items = entries.map(([key, val]) => {
            const label = key.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase());
            return '<div class="field-item"><div class="field-label">' + this._esc(label) +
                '</div><input class="edit-input" name="' + this._esc(key) + '" value="' + this._esc(val) + '" /></div>';
        }).join("");

        content.innerHTML =
            '<div class="card">' +
            '<div style="display:flex;gap:0.5rem;margin-bottom:1rem;">' +
            '<button class="btn-save" id="save-btn">Save Changes</button>' +
            '<button class="btn-cancel" id="cancel-btn">Cancel</button>' +
            '</div>' +
            '<div class="field-grid">' + items + '</div>' +
            '<div class="success" id="save-msg" style="display:none;"></div>' +
            '</div>';

        content.querySelector("#cancel-btn").addEventListener("click", () => {
            this._editing = false;
            this._renderContent();
        });

        content.querySelector("#save-btn").addEventListener("click", async () => {
            const inputs = content.querySelectorAll(".edit-input");
            const fields = {};
            let changed = false;

            const originalFields = this._asset.fields;
            inputs.forEach(input => {
                const val = input.value.trim();
                if (val && val !== originalFields[input.name]) {
                    fields[input.name] = val;
                    changed = true;
                }
            });

            if (!changed) {
                this._editing = false;
                this._renderContent();
                return;
            }

            try {
                const resp = await fetch("/api/assets/" + this._assetId + "/fields", {
                    method: "PUT",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ fields }),
                });
                if (!resp.ok) throw new Error("Save failed");

                this._editing = false;
                await this._load(); // Reload to show updated values + new mutations
            } catch (e) {
                const msg = content.querySelector("#save-msg");
                msg.style.display = "block";
                msg.style.color = "#e53e3e";
                msg.textContent = "Error: " + e.message;
            }
        });
    }

    _renderQuality(content) {
        if (!this._quality) {
            content.innerHTML = '<div class="card"><div class="empty">Quality data unavailable.</div></div>';
            return;
        }
        const q = this._quality;

        const scoreColor = (s) => s >= 0.8 ? "score-green" : s >= 0.5 ? "score-yellow" : "score-red";
        const pct = (s) => Math.round(s * 100) + "%";

        let html = '<div class="quality-grid">';
        html += '<div class="quality-card"><h3>Completeness</h3><div class="score-value ' + scoreColor(q.completeness.score) + '">' + pct(q.completeness.score) + '</div>';
        html += '<div class="score-label">' + q.completeness.required_populated + '/' + q.completeness.required_total + ' required, ' + q.completeness.recommended_populated + '/' + q.completeness.recommended_total + ' recommended</div></div>';
        html += '<div class="quality-card"><h3>Accuracy</h3><div class="score-value ' + scoreColor(q.accuracy.score) + '">' + pct(q.accuracy.score) + '</div>';
        html += '<div class="score-label">' + q.accuracy.rules_passed + '/' + q.accuracy.rules_evaluated + ' rules passed</div></div>';
        html += '<div class="quality-card"><h3>Recency</h3><div class="score-value ' + scoreColor(q.recency.score) + '">' + pct(q.recency.score) + '</div>';
        const staleCount = q.recency.tracked_fields.filter(f => f.is_stale).length;
        html += '<div class="score-label">' + staleCount + ' of ' + q.recency.tracked_fields.length + ' tracked fields stale</div></div>';
        html += '</div>';

        // Composite
        html += '<div class="card" style="margin-bottom:1rem;"><strong>Composite Score: </strong><span class="' + scoreColor(q.composite) + '" style="font-size:1.125rem;font-weight:700;">' + pct(q.composite) + '</span></div>';

        // Gaps
        const gaps = [];
        for (const f of q.completeness.missing_required) {
            gaps.push('<div class="gap-item"><span class="gap-icon">\u2717</span> Missing required: <strong>' + this._esc(f.replace(/_/g, " ")) + '</strong></div>');
        }
        for (const f of q.completeness.missing_recommended) {
            gaps.push('<div class="gap-item"><span class="gap-warn">\u26A0</span> Missing recommended: ' + this._esc(f.replace(/_/g, " ")) + '</div>');
        }
        for (const f of q.accuracy.failures) {
            gaps.push('<div class="gap-item"><span class="gap-icon">\u2717</span> ' + this._esc(f.description) + '</div>');
        }
        for (const f of q.recency.tracked_fields.filter(f => f.is_stale)) {
            const days = f.days_since_update !== null ? f.days_since_update + " days old" : "never updated";
            gaps.push('<div class="gap-item"><span class="gap-warn">\u26A0</span> Stale: ' + this._esc(f.field_name.replace(/_/g, " ")) + ' (' + days + ', threshold: ' + f.threshold_days + ' days)</div>');
        }

        if (gaps.length > 0) {
            html += '<div class="card"><h3 style="font-size:0.875rem;font-weight:600;margin:0 0 0.75rem;">Data Gaps</h3><div class="gap-list">' + gaps.join("") + '</div></div>';
        }

        content.innerHTML = html;
    }

    _renderHistory(content) {
        if (this._mutations.length === 0) {
            content.innerHTML = '<div class="card"><div class="empty">No mutation history.</div></div>';
            return;
        }
        const rows = this._mutations.map(m => {
            const stateCls = "state-" + m.approval_state.toLowerCase();
            const label = m.field_name.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase());
            return '<tr><td>' + this._esc(label) + '</td><td>' + this._esc(m.value) + '</td>' +
                '<td>' + this._esc(m.effective_date) + '</td>' +
                '<td class="' + stateCls + '">' + this._esc(m.approval_state) + '</td>' +
                '<td>' + this._esc(m.submitted_at.substring(0, 19)) + '</td></tr>';
        }).join("");

        content.innerHTML = '<div class="card"><table><thead><tr>' +
            '<th>Field</th><th>Value</th><th>Effective Date</th><th>Status</th><th>Submitted</th>' +
            '</tr></thead><tbody>' + rows + '</tbody></table></div>';
    }

    _esc(str) { const d = document.createElement("div"); d.textContent = str || ""; return d.innerHTML; }
}

customElements.define("centurisk-asset-detail", CenturiskAssetDetail);
