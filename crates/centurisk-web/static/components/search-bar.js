/**
 * <centurisk-search-bar> — NL search bar with results dropdown.
 * Try: "buildings over $5M", "fire station", "vehicles in Springfield"
 */

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host { display: block; margin-bottom: 1.5rem; }
    .search-container { position: relative; }
    .search-input {
        width: 100%;
        padding: 0.75rem 1rem 0.75rem 2.5rem;
        border: 1px solid var(--color-border, #e2e8f0);
        border-radius: 8px;
        font-size: 0.9375rem;
        font-family: inherit;
        background: #fff;
        box-shadow: 0 1px 2px rgba(0,0,0,0.05);
    }
    .search-input:focus { outline: none; border-color: var(--color-primary, #1a365d); box-shadow: 0 0 0 3px rgba(26,54,93,0.1); }
    .search-input::placeholder { color: #a0aec0; }
    .search-icon { position: absolute; left: 0.75rem; top: 50%; transform: translateY(-50%); color: #a0aec0; font-size: 1rem; pointer-events: none; }

    .results-panel {
        display: none;
        position: absolute;
        top: 100%;
        left: 0;
        right: 0;
        background: #fff;
        border: 1px solid var(--color-border, #e2e8f0);
        border-radius: 8px;
        box-shadow: 0 4px 12px rgba(0,0,0,0.1);
        max-height: 400px;
        overflow-y: auto;
        z-index: 100;
        margin-top: 4px;
    }
    .results-panel.open { display: block; }

    .result-item {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        padding: 0.75rem 1rem;
        cursor: pointer;
        border-bottom: 1px solid #f0f0f0;
    }
    .result-item:hover { background: #f7fafc; }
    .result-item:last-child { border-bottom: none; }
    .result-name { font-weight: 500; font-size: 0.875rem; color: var(--color-text, #2d3748); }
    .result-meta { font-size: 0.75rem; color: var(--color-text-muted, #718096); }
    .result-snippet { font-size: 0.75rem; color: #a0aec0; margin-top: 0.125rem; }
    .result-badge { display: inline-block; padding: 0.0625rem 0.375rem; border-radius: 4px; font-size: 0.625rem; font-weight: 600; background: #ebf4ff; color: #2b6cb0; }

    .query-info { padding: 0.5rem 1rem; font-size: 0.75rem; color: var(--color-text-muted, #718096); border-bottom: 1px solid #f0f0f0; }
    .suggestion { padding: 0.5rem 1rem; font-size: 0.8125rem; color: #3182ce; cursor: pointer; }
    .suggestion:hover { background: #ebf8ff; }
    .no-results { padding: 1.5rem 1rem; text-align: center; color: var(--color-text-muted, #718096); font-size: 0.875rem; }
</style>
<div class="search-container">
    <span class="search-icon">\u{1F50D}</span>
    <input class="search-input" type="text" placeholder="Search exposures... try &quot;buildings over $5M&quot; or &quot;fire station&quot;" id="search-input" />
    <div class="results-panel" id="results-panel"></div>
</div>
`;

class CenturiskSearchBar extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
    }

    connectedCallback() {
        let debounce;
        const input = this.shadowRoot.getElementById("search-input");
        input.addEventListener("input", () => {
            clearTimeout(debounce);
            debounce = setTimeout(() => this._search(input.value), 300);
        });
        input.addEventListener("focus", () => {
            if (input.value.length > 1) this._search(input.value);
        });
        // Close on outside click
        document.addEventListener("click", (e) => {
            if (!this.contains(e.target)) {
                this.shadowRoot.getElementById("results-panel").classList.remove("open");
            }
        });
    }

    async _search(query) {
        const panel = this.shadowRoot.getElementById("results-panel");
        if (query.length < 2) { panel.classList.remove("open"); return; }

        try {
            const resp = await fetch("/api/search?q=" + encodeURIComponent(query));
            if (!resp.ok) return;
            const data = await resp.json();
            this._renderResults(data);
        } catch (_) {}
    }

    _renderResults(data) {
        const panel = this.shadowRoot.getElementById("results-panel");
        let html = '';

        // Query interpretation
        const parts = [];
        if (data.query.asset_type) parts.push("Type: " + data.query.asset_type);
        if (data.query.search_text) parts.push("Text: " + data.query.search_text);
        for (const f of data.query.numeric_filters) {
            parts.push(f.field.replace(/_/g, " ") + " " + f.op + " $" + f.value.toLocaleString());
        }
        if (parts.length > 0) {
            html += '<div class="query-info">Interpreted: ' + parts.join(" + ") + ' (confidence: ' + Math.round(data.query.confidence * 100) + '%)</div>';
        }

        // Suggestions
        if (data.query.suggestions.length > 0) {
            for (const s of data.query.suggestions) {
                html += '<div class="suggestion" data-suggestion="' + this._esc(s.replace("Try: ", "")) + '">' + this._esc(s) + '</div>';
            }
        }

        // Results
        if (data.results.length === 0) {
            html += '<div class="no-results">No matching exposures found.</div>';
        } else {
            for (const r of data.results) {
                html += '<div class="result-item" data-id="' + r.asset_id + '">' +
                    '<div><div class="result-name">' + this._esc(r.snippet || r.asset_id.substring(0, 8)) + '</div>' +
                    '<div class="result-meta"><span class="result-badge">' + this._esc(r.asset_type) + '</span></div></div></div>';
            }
        }

        panel.innerHTML = html;
        panel.classList.add("open");

        // Wire click events
        panel.querySelectorAll(".result-item").forEach(el => {
            el.addEventListener("click", () => {
                panel.classList.remove("open");
                this.dispatchEvent(new CustomEvent("navigate", {
                    detail: { page: "asset-detail", assetId: el.dataset.id },
                    bubbles: true, composed: true,
                }));
            });
        });
        panel.querySelectorAll(".suggestion").forEach(el => {
            el.addEventListener("click", () => {
                this.shadowRoot.getElementById("search-input").value = el.dataset.suggestion;
                this._search(el.dataset.suggestion);
            });
        });
    }

    _esc(str) { const d = document.createElement("div"); d.textContent = str || ""; return d.innerHTML; }
}

customElements.define("centurisk-search-bar", CenturiskSearchBar);
