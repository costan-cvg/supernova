import "./nav-sidebar.js";

const PAGE_TITLES = {
    dashboard: "Dashboard",
    assets: "Assets",
    quality: "Quality",
    approvals: "Approvals",
    reports: "Reports",
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

    .page-title {
        font-size: 1.125rem;
        font-weight: 600;
        color: var(--color-text, #2d3748);
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

    /* Mobile: collapsible sidebar */
    .overlay {
        display: none;
    }

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

        .sidebar.open {
            transform: translateX(0);
        }

        .overlay {
            display: none;
            position: fixed;
            inset: 0;
            background: rgba(0, 0, 0, 0.4);
            z-index: 90;
        }

        .overlay.open {
            display: block;
        }

        .main {
            margin-left: 0;
        }

        .menu-btn {
            display: block;
        }
    }
</style>

<div class="overlay"></div>
<div class="sidebar">
    <centurisk-nav></centurisk-nav>
</div>
<div class="main">
    <header class="header">
        <button class="menu-btn" aria-label="Open menu">\u2630</button>
        <h1 class="page-title">Dashboard</h1>
        <div></div>
    </header>
    <div class="content">
        <div class="placeholder">Select a section from the sidebar to get started.</div>
    </div>
</div>
`;

class CenturiskApp extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
    }

    connectedCallback() {
        this.shadowRoot.addEventListener("nav-change", (e) => {
            this._setPage(e.detail.id);
            this._closeMobileMenu();
        });

        const menuBtn = this.shadowRoot.querySelector(".menu-btn");
        menuBtn.addEventListener("click", () => this._toggleMobileMenu());

        const overlay = this.shadowRoot.querySelector(".overlay");
        overlay.addEventListener("click", () => this._closeMobileMenu());
    }

    _setPage(id) {
        const title = PAGE_TITLES[id] || id;
        this.shadowRoot.querySelector(".page-title").textContent = title;
        this.shadowRoot.querySelector(".placeholder").textContent =
            `${title} content will appear here.`;
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
