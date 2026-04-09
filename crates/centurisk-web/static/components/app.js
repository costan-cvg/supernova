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
    assets: "Exposures",
    quality: "Quality",
    approvals: "Approvals",
    reports: "Reports",
    "asset-create": "Add Exposure",
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

    .notification-bell {
        position: relative;
        background: none;
        border: none;
        font-size: 1.25rem;
        cursor: pointer;
        padding: 0.25rem 0.375rem;
        border-radius: 4px;
        line-height: 1;
    }

    .notification-bell:hover { background: #f7fafc; }

    .notification-badge {
        position: absolute;
        top: -4px;
        right: -6px;
        background: #e53e3e;
        color: #fff;
        font-size: 0.625rem;
        font-weight: 700;
        min-width: 16px;
        height: 16px;
        line-height: 16px;
        text-align: center;
        border-radius: 8px;
        padding: 0 3px;
    }

    .notification-panel {
        display: none;
        position: absolute;
        top: 48px;
        right: 0;
        width: 360px;
        max-height: 420px;
        overflow-y: auto;
        background: #fff;
        border: 1px solid var(--color-border, #e2e8f0);
        border-radius: 8px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
        z-index: 200;
    }

    .notification-panel.open { display: block; }

    .notification-panel-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0.75rem 1rem;
        border-bottom: 1px solid var(--color-border, #e2e8f0);
        font-weight: 600;
        font-size: 0.875rem;
    }

    .notification-panel-header button {
        background: none;
        border: none;
        color: #3182ce;
        font-size: 0.75rem;
        cursor: pointer;
        padding: 0.125rem 0.25rem;
    }

    .notification-panel-header button:hover { text-decoration: underline; }

    .notification-item {
        display: flex;
        align-items: flex-start;
        gap: 0.625rem;
        padding: 0.75rem 1rem;
        border-bottom: 1px solid #f0f0f0;
    }

    .notification-item.unread { background: #ebf8ff; }

    .notification-item-content { flex: 1; min-width: 0; }

    .notification-item-title {
        font-size: 0.8125rem;
        font-weight: 600;
        color: var(--color-text, #2d3748);
        margin-bottom: 0.125rem;
    }

    .notification-item-body {
        font-size: 0.75rem;
        color: var(--color-text-muted, #718096);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .notification-item-time {
        font-size: 0.625rem;
        color: #a0aec0;
        margin-top: 0.125rem;
    }

    .notification-item-ack {
        background: none;
        border: 1px solid #cbd5e0;
        border-radius: 4px;
        font-size: 0.625rem;
        cursor: pointer;
        padding: 0.125rem 0.375rem;
        color: #718096;
        flex-shrink: 0;
        align-self: center;
    }

    .notification-item-ack:hover { background: #f7fafc; border-color: #a0aec0; }

    .notification-empty {
        padding: 2rem 1rem;
        text-align: center;
        color: var(--color-text-muted, #718096);
        font-size: 0.8125rem;
    }

    .notification-priority-urgent .notification-item-title { color: #e53e3e; }
    .notification-priority-high .notification-item-title { color: #dd6b20; }

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
            '      <span style="position:relative">' +
            '        <button class="notification-bell" id="bell-btn" aria-label="Notifications">' +
            '          \uD83D\uDD14<span class="notification-badge" id="bell-badge" style="display:none">0</span>' +
            '        </button>' +
            '        <div class="notification-panel" id="notification-panel">' +
            '          <div class="notification-panel-header">' +
            '            <span>Notifications</span>' +
            '            <button id="ack-all-btn">Mark all read</button>' +
            '          </div>' +
            '          <div id="notification-list"></div>' +
            '        </div>' +
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

        // Notification bell
        root.querySelector("#bell-btn").addEventListener("click", (e) => {
            e.stopPropagation();
            this._toggleNotificationPanel();
        });
        root.querySelector("#ack-all-btn").addEventListener("click", () => this._acknowledgeAll());

        // Close panel when clicking outside
        this.shadowRoot.addEventListener("click", (e) => {
            const panel = this.shadowRoot.getElementById("notification-panel");
            if (panel && panel.classList.contains("open")) {
                const bellBtn = this.shadowRoot.getElementById("bell-btn");
                if (!panel.contains(e.target) && e.target !== bellBtn) {
                    panel.classList.remove("open");
                }
            }
        });

        // Start polling for notification count
        this._fetchNotificationCount();
        this._notificationPollId = setInterval(() => this._fetchNotificationCount(), 30000);

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
                if (titleEl) titleEl.textContent = "Exposure Detail";
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
        if (this._notificationPollId) {
            clearInterval(this._notificationPollId);
            this._notificationPollId = null;
        }
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

    disconnectedCallback() {
        if (this._notificationPollId) {
            clearInterval(this._notificationPollId);
            this._notificationPollId = null;
        }
    }

    _authHeaders() {
        const token = localStorage.getItem("centurisk_token");
        const headers = { "Content-Type": "application/json" };
        if (token) headers["Authorization"] = "Bearer " + token;
        return headers;
    }

    async _fetchNotificationCount() {
        try {
            const resp = await fetch("/api/notifications/count", { headers: this._authHeaders() });
            if (!resp.ok) return;
            const data = await resp.json();
            const badge = this.shadowRoot.getElementById("bell-badge");
            if (badge) {
                if (data.unread > 0) {
                    badge.textContent = data.unread > 99 ? "99+" : String(data.unread);
                    badge.style.display = "";
                } else {
                    badge.style.display = "none";
                }
            }
        } catch (_) { /* ignore fetch errors */ }
    }

    _toggleNotificationPanel() {
        const panel = this.shadowRoot.getElementById("notification-panel");
        if (!panel) return;
        const isOpen = panel.classList.toggle("open");
        if (isOpen) this._fetchNotifications();
    }

    async _fetchNotifications() {
        try {
            const resp = await fetch("/api/notifications", { headers: this._authHeaders() });
            if (!resp.ok) return;
            const notifications = await resp.json();
            this._renderNotificationList(notifications);
        } catch (_) { /* ignore */ }
    }

    _renderNotificationList(notifications) {
        const listEl = this.shadowRoot.getElementById("notification-list");
        if (!listEl) return;

        if (notifications.length === 0) {
            listEl.innerHTML = '<div class="notification-empty">No notifications</div>';
            return;
        }

        listEl.innerHTML = notifications.map(n => {
            const isUnread = n.state !== "Acknowledged";
            const priorityClass = (n.priority === "Urgent" || n.priority === "High")
                ? " notification-priority-" + n.priority.toLowerCase() : "";
            const timeAgo = this._timeAgo(n.created_at);
            return '<div class="notification-item' + (isUnread ? " unread" : "") + priorityClass + '" data-id="' + this._esc(n.notification_id) + '">' +
                '<div class="notification-item-content">' +
                '  <div class="notification-item-title">' + this._esc(n.title) + '</div>' +
                '  <div class="notification-item-body">' + this._esc(n.body) + '</div>' +
                '  <div class="notification-item-time">' + this._esc(timeAgo) + '</div>' +
                '</div>' +
                (isUnread ? '<button class="notification-item-ack" data-nid="' + this._esc(n.notification_id) + '">ack</button>' : '') +
                '</div>';
        }).join("");

        // Bind ack buttons
        listEl.querySelectorAll(".notification-item-ack").forEach(btn => {
            btn.addEventListener("click", (e) => {
                e.stopPropagation();
                this._acknowledgeOne(btn.getAttribute("data-nid"));
            });
        });
    }

    async _acknowledgeOne(notificationId) {
        try {
            await fetch("/api/notifications/" + notificationId + "/acknowledge", {
                method: "POST",
                headers: this._authHeaders(),
            });
            this._fetchNotificationCount();
            this._fetchNotifications();
        } catch (_) { /* ignore */ }
    }

    async _acknowledgeAll() {
        try {
            await fetch("/api/notifications/acknowledge-all", {
                method: "POST",
                headers: this._authHeaders(),
            });
            this._fetchNotificationCount();
            this._fetchNotifications();
        } catch (_) { /* ignore */ }
    }

    _timeAgo(isoStr) {
        try {
            const then = new Date(isoStr);
            const now = new Date();
            const diffMs = now - then;
            const mins = Math.floor(diffMs / 60000);
            if (mins < 1) return "just now";
            if (mins < 60) return mins + "m ago";
            const hrs = Math.floor(mins / 60);
            if (hrs < 24) return hrs + "h ago";
            const days = Math.floor(hrs / 24);
            return days + "d ago";
        } catch (_) { return ""; }
    }

    _esc(str) {
        const div = document.createElement("div");
        div.textContent = str || "";
        return div.innerHTML;
    }
}

customElements.define("centurisk-app", CenturiskApp);
