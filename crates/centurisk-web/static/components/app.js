import "./nav-sidebar.js";
import "./asset-list.js";
import "./asset-form.js";

const PAGE_TITLES = {
    dashboard: "Dashboard",
    assets: "Assets",
    quality: "Quality",
    approvals: "Approvals",
    reports: "Reports",
    "asset-create": "Add Asset",
};

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host {
        display: flex;
        min-height: 100vh;
    }

    .sidebar {
        width: 240px;
        flex-shrink: 0;
        position: fixed;
        top: 0;
        left: 0;
        bottom: 0;
        z-index: 100;
    }

    .main {
        flex: 1;
        margin-left: 240px;
        display: flex;
        flex-direction: column;
        min-height: 100vh;
    }

    .header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        height: 56px;
        padding: 0 2rem;
        background: #fff;
        border-bottom: 1px solid var(--color-border, #e2e8f0);
        box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
    }

    .header-left {
        display: flex;
        align-items: center;
        gap: 1rem;
    }

    .page-title {
        font-size: 1.125rem;
        font-weight: 600;
        color: var(--color-text, #2d3748);
    }

    .user-info {
        font-size: 0.8125rem;
        color: var(--color-text-muted, #718096);
    }

    .content {
        flex: 1;
        padding: 2rem;
    }

    .placeholder {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 300px;
        background: #fff;
        border: 1px dashed var(--color-border, #e2e8f0);
        border-radius: 6px;
        color: var(--color-text-muted, #718096);
        font-size: 0.9rem;
    }

    .overlay { display: none; }
    .menu-btn {
        display: none;
        background: none;
        border: none;
        font-size: 1.25rem;
        cursor: pointer;
        padding: 0.25rem;
        color: var(--color-text, #2d3748);
    }

    @media (max-width: 768px) {
        .sidebar {
            transform: translateX(-100%);
            transition: transform 0.2s ease;
        }
        .sidebar.open { transform: translateX(0); }
        .overlay { display: none; position: fixed; inset: 0; background: rgba(0,0,0,0.4); z-index: 90; }
        .overlay.open { display: block; }
        .main { margin-left: 0; }
        .menu-btn { display: block; }
    }
</style>

<div class="overlay"></div>
<div class="sidebar">
    <centurisk-nav></centurisk-nav>
</div>
<div class="main">
    <header class="header">
        <div class="header-left">
            <button class="menu-btn" aria-label="Open menu">\u2630</button>
            <h1 class="page-title">Dashboard</h1>
        </div>
        <span class="user-info" id="user-info"></span>
    </header>
    <div class="content" id="content">
        <div class="placeholder">Select a section from the sidebar to get started.</div>
    </div>
</div>
`;

class CenturiskApp extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._currentPage = "dashboard";
    }

    connectedCallback() {
        this.shadowRoot.addEventListener("nav-change", (e) => {
            this._setPage(e.detail.id);
            this._closeMobileMenu();
        });

        this.shadowRoot.addEventListener("navigate", (e) => {
            this._setPage(e.detail.page, e.detail);
        });

        this.shadowRoot.querySelector(".menu-btn").addEventListener("click", () => this._toggleMobileMenu());
        this.shadowRoot.querySelector(".overlay").addEventListener("click", () => this._closeMobileMenu());

        this._loadUser();
    }

    async _loadUser() {
        try {
            const resp = await fetch("/api/me");
            if (resp.ok) {
                const user = await resp.json();
                this.shadowRoot.getElementById("user-info").textContent =
                    `Logged in as: ${user.display_name}`;
            }
        } catch (_) {}
    }

    _setPage(id, detail) {
        this._currentPage = id;
        const title = PAGE_TITLES[id] || id;
        this.shadowRoot.querySelector(".page-title").textContent = title;

        const content = this.shadowRoot.getElementById("content");

        switch (id) {
            case "assets":
                content.innerHTML = "";
                content.appendChild(document.createElement("centurisk-asset-list"));
                break;
            case "asset-create":
                content.innerHTML = "";
                content.appendChild(document.createElement("centurisk-asset-form"));
                break;
            default:
                content.innerHTML = `<div class="placeholder">${title} content will appear here.</div>`;
                break;
        }
    }

    _toggleMobileMenu() {
        const sidebar = this.shadowRoot.querySelector(".sidebar");
        const overlay = this.shadowRoot.querySelector(".overlay");
        const isOpen = sidebar.classList.contains("open");
        sidebar.classList.toggle("open", !isOpen);
        overlay.classList.toggle("open", !isOpen);
    }

    _closeMobileMenu() {
        this.shadowRoot.querySelector(".sidebar").classList.remove("open");
        this.shadowRoot.querySelector(".overlay").classList.remove("open");
    }
}

customElements.define("centurisk-app", CenturiskApp);
