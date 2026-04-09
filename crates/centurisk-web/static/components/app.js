import "./nav-sidebar.js";
import "./asset-list.js";
import "./asset-form.js";
import "./asset-detail.js";
import "./approval-queue.js";
import "./dashboard.js";
import "./renewal-page.js";
import "./login-page.js";

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

    .header-right {
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

    .role-badge {
        display: inline-block;
        padding: 0.125rem 0.5rem;
        border-radius: 4px;
        font-size: 0.6875rem;
        font-weight: 600;
        margin-left: 0.375rem;
    }

    .role-centurisk { background: #ebf4ff; color: #2b6cb0; }
    .role-pool { background: #c6f6d5; color: #276749; }
    .role-member { background: #fefcbf; color: #975a16; }

    .btn-logout {
        padding: 0.25rem 0.5rem;
        background: none;
        border: 1px solid var(--color-border, #e2e8f0);
        border-radius: 4px;
        font-size: 0.75rem;
        cursor: pointer;
        color: var(--color-text-muted, #718096);
    }

    .btn-logout:hover { background: #f7fafc; }

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
        .sidebar { transform: translateX(-100%); transition: transform 0.2s ease; }
        .sidebar.open { transform: translateX(0); }
        .overlay { display: none; position: fixed; inset: 0; background: rgba(0,0,0,0.4); z-index: 90; }
        .overlay.open { display: block; }
        .main { margin-left: 0; }
        .menu-btn { display: block; }
    }
</style>

<div id="root"></div>
`;

const ROLE_CLASS_MAP = {
    CentuRiskAdmin: "role-centurisk",
    CentuRiskAnalyst: "role-centurisk",
    CentuRiskAuditor: "role-centurisk",
    CentuRiskSupport: "role-centurisk",
    PoolAdministrator: "role-pool",
    PoolAnalyst: "role-pool",
    PoolReadOnly: "role-pool",
    MemberAdmin: "role-member",
    MemberUser: "role-member",
    MemberReadOnly: "role-member",
};

class CenturiskApp extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._currentPage = "dashboard";
        this._user = null;
    }

    connectedCallback() {
        this._checkSession();
    }

    _checkSession() {
        const token = localStorage.getItem("centurisk_token");
        const userJson = localStorage.getItem("centurisk_user");

        if (token && userJson) {
            try {
                this._user = JSON.parse(userJson);
                this._renderApp();
                return;
            } catch (_) {}
        }

        this._renderLogin();
    }

    _renderLogin() {
        const root = this.shadowRoot.getElementById("root");
        root.innerHTML = "";
        const login = document.createElement("centurisk-login");
        root.appendChild(login);
    }

    _renderApp() {
        const root = this.shadowRoot.getElementById("root");
        const u = this._user;
        const roleClass = ROLE_CLASS_MAP[u.category] || "role-member";

        root.innerHTML =
            '<div class="overlay"></div>' +
            '<div class="sidebar"><centurisk-nav></centurisk-nav></div>' +
            '<div class="main">' +
            '  <header class="header">' +
            '    <div class="header-left">' +
            '      <button class="menu-btn" aria-label="Open menu">\u2630</button>' +
            '      <h1 class="page-title">Dashboard</h1>' +
            '    </div>' +
            '    <div class="header-right">' +
            '      <span class="user-info">' + this._esc(u.display_name) +
            '        <span class="role-badge ' + roleClass + '">' + this._esc(u.category) + '</span>' +
            '      </span>' +
            '      <button class="btn-logout" id="logout-btn">Log out</button>' +
            '    </div>' +
            '  </header>' +
            '  <div class="content" id="content">' +
            '    <centurisk-dashboard></centurisk-dashboard>' +
            '  </div>' +
            '</div>';

        // Event listeners
        root.querySelector(".overlay").addEventListener("click", () => this._closeMobileMenu());
        root.querySelector(".menu-btn").addEventListener("click", () => this._toggleMobileMenu());
        root.querySelector("#logout-btn").addEventListener("click", () => this._logout());

        this.shadowRoot.addEventListener("nav-change", (e) => {
            this._setPage(e.detail.id);
            this._closeMobileMenu();
        });

        this.shadowRoot.addEventListener("navigate", (e) => {
            this._setPage(e.detail.page, e.detail);
        });
    }

    _setPage(id, detail) {
        this._currentPage = id;
        const title = PAGE_TITLES[id] || id;
        const titleEl = this.shadowRoot.querySelector(".page-title");
        if (titleEl) titleEl.textContent = title;

        const content = this.shadowRoot.getElementById("content");
        if (!content) return;

        switch (id) {
            case "dashboard":
                content.innerHTML = "";
                content.appendChild(document.createElement("centurisk-dashboard"));
                break;
            case "assets":
                content.innerHTML = "";
                content.appendChild(document.createElement("centurisk-asset-list"));
                break;
            case "asset-create":
                content.innerHTML = "";
                content.appendChild(document.createElement("centurisk-asset-form"));
                break;
            case "asset-detail": {
                content.innerHTML = "";
                const detailEl = document.createElement("centurisk-asset-detail");
                content.appendChild(detailEl);
                if (detail && detail.assetId) detailEl.assetId = detail.assetId;
                if (titleEl) titleEl.textContent = "Asset Detail";
                break;
            }
            case "approvals":
                content.innerHTML = "";
                content.appendChild(document.createElement("centurisk-approval-queue"));
                break;
            case "reports":
                content.innerHTML = "";
                content.appendChild(document.createElement("centurisk-renewal-page"));
                if (titleEl) titleEl.textContent = "Renewals";
                break;
            default:
                content.innerHTML = '<div class="placeholder">' + title + ' content will appear here.</div>';
                break;
        }
    }

    _logout() {
        localStorage.removeItem("centurisk_token");
        localStorage.removeItem("centurisk_user");
        document.cookie = "centurisk_session=;path=/;expires=Thu, 01 Jan 1970 00:00:00 GMT";
        this._user = null;
        this._renderLogin();
    }

    _toggleMobileMenu() {
        const sidebar = this.shadowRoot.querySelector(".sidebar");
        const overlay = this.shadowRoot.querySelector(".overlay");
        if (sidebar) {
            const isOpen = sidebar.classList.contains("open");
            sidebar.classList.toggle("open", !isOpen);
            if (overlay) overlay.classList.toggle("open", !isOpen);
        }
    }

    _closeMobileMenu() {
        const sidebar = this.shadowRoot.querySelector(".sidebar");
        const overlay = this.shadowRoot.querySelector(".overlay");
        if (sidebar) sidebar.classList.remove("open");
        if (overlay) overlay.classList.remove("open");
    }

    _esc(str) {
        const div = document.createElement("div");
        div.textContent = str || "";
        return div.innerHTML;
    }
}

customElements.define("centurisk-app", CenturiskApp);
