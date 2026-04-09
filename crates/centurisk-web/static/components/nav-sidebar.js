const NAV_ITEMS = [
    { id: "dashboard", label: "Dashboard", icon: "\u25A6" },
    { id: "assets", label: "Exposures", icon: "\u2302" },
    { id: "quality", label: "Quality", icon: "\u2261" },
    { id: "approvals", label: "Approvals", icon: "\u2713" },
    { id: "reports", label: "Reports", icon: "\u2637" },
];

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: var(--color-sidebar, #2d3748);
        color: #fff;
    }

    .logo {
        display: flex;
        align-items: center;
        gap: 0.625rem;
        padding: 1rem 1.25rem;
        border-bottom: 1px solid rgba(255, 255, 255, 0.08);
    }

    .logo-icon {
        width: 32px;
        height: 32px;
        background: var(--color-primary, #1a365d);
        border-radius: 6px;
        display: flex;
        align-items: center;
        justify-content: center;
        font-weight: 700;
        font-size: 0.875rem;
        letter-spacing: -0.5px;
        flex-shrink: 0;
    }

    .logo-text {
        font-size: 1.125rem;
        font-weight: 600;
        letter-spacing: -0.25px;
    }

    nav {
        flex: 1;
        padding: 0.5rem 0;
    }

    .nav-item {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        padding: 0.625rem 1.25rem;
        font-size: 0.9rem;
        color: rgba(255, 255, 255, 0.72);
        cursor: pointer;
        transition: background 0.15s ease, color 0.15s ease;
        border: none;
        background: none;
        width: 100%;
        text-align: left;
        font-family: inherit;
    }

    .nav-item:hover {
        background: rgba(255, 255, 255, 0.06);
        color: #fff;
    }

    .nav-item[aria-current="true"] {
        background: rgba(255, 255, 255, 0.12);
        color: #fff;
        font-weight: 500;
    }

    .nav-icon {
        width: 20px;
        text-align: center;
        font-size: 1rem;
        flex-shrink: 0;
    }
</style>

<div class="logo">
    <div class="logo-icon">RS</div>
    <span class="logo-text">RiskStar</span>
</div>
<nav></nav>
`;

export class CenturiskNav extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._active = "dashboard";
    }

    connectedCallback() {
        this._renderNav();
    }

    get active() {
        return this._active;
    }

    set active(value) {
        if (this._active !== value) {
            this._active = value;
            this._updateActive();
        }
    }

    _renderNav() {
        const nav = this.shadowRoot.querySelector("nav");
        nav.innerHTML = "";
        for (const item of NAV_ITEMS) {
            const btn = document.createElement("button");
            btn.className = "nav-item";
            btn.dataset.id = item.id;
            btn.setAttribute(
                "aria-current",
                item.id === this._active ? "true" : "false"
            );
            btn.innerHTML = `<span class="nav-icon">${item.icon}</span>${item.label}`;
            btn.addEventListener("click", () => this._onItemClick(item.id));
            nav.appendChild(btn);
        }
    }

    _onItemClick(id) {
        this._active = id;
        this._updateActive();
        this.dispatchEvent(
            new CustomEvent("nav-change", {
                detail: { id },
                bubbles: true,
                composed: true,
            })
        );
    }

    _updateActive() {
        const items = this.shadowRoot.querySelectorAll(".nav-item");
        for (const item of items) {
            item.setAttribute(
                "aria-current",
                item.dataset.id === this._active ? "true" : "false"
            );
        }
    }
}

customElements.define("centurisk-nav", CenturiskNav);
