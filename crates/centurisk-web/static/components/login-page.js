/**
 * <centurisk-login> — Login page with user/role selector.
 * Fetches users from GET /api/users, logs in via POST /api/login.
 */

const template = document.createElement("template");
template.innerHTML = `
<style>
    :host {
        display: flex;
        align-items: center;
        justify-content: center;
        min-height: 100vh;
        background: var(--color-bg, #f7fafc);
    }

    .login-card {
        background: #fff;
        border-radius: 8px;
        box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
        padding: 2.5rem;
        width: 100%;
        max-width: 400px;
    }

    .logo {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        margin-bottom: 2rem;
    }

    .logo-icon {
        width: 40px;
        height: 40px;
        background: var(--color-primary, #1a365d);
        border-radius: 8px;
        display: flex;
        align-items: center;
        justify-content: center;
        color: #fff;
        font-weight: 700;
        font-size: 1rem;
    }

    .logo-text {
        font-size: 1.5rem;
        font-weight: 700;
        color: var(--color-text, #2d3748);
    }

    .subtitle {
        font-size: 0.875rem;
        color: var(--color-text-muted, #718096);
        margin-bottom: 1.5rem;
    }

    label {
        display: block;
        font-size: 0.8125rem;
        font-weight: 500;
        color: var(--color-text, #2d3748);
        margin-bottom: 0.375rem;
    }

    select {
        width: 100%;
        padding: 0.625rem 0.75rem;
        border: 1px solid var(--color-border, #e2e8f0);
        border-radius: 6px;
        font-size: 0.9375rem;
        font-family: inherit;
        color: var(--color-text, #2d3748);
        background: #fff;
        margin-bottom: 1.25rem;
        cursor: pointer;
    }

    select:focus {
        outline: none;
        border-color: var(--color-primary, #1a365d);
        box-shadow: 0 0 0 3px rgba(26, 54, 93, 0.1);
    }

    .user-detail {
        background: #f7fafc;
        border-radius: 6px;
        padding: 0.75rem 1rem;
        margin-bottom: 1.25rem;
        font-size: 0.8125rem;
        color: var(--color-text-muted, #718096);
        min-height: 3rem;
    }

    .user-detail .role-badge {
        display: inline-block;
        padding: 0.125rem 0.5rem;
        border-radius: 4px;
        font-size: 0.75rem;
        font-weight: 600;
        margin-bottom: 0.375rem;
    }

    .role-centurisk { background: #ebf4ff; color: #2b6cb0; }
    .role-pool { background: #c6f6d5; color: #276749; }
    .role-member { background: #fefcbf; color: #975a16; }

    .btn-login {
        width: 100%;
        padding: 0.625rem;
        background: var(--color-primary, #1a365d);
        color: #fff;
        border: none;
        border-radius: 6px;
        font-size: 0.9375rem;
        font-weight: 600;
        cursor: pointer;
        transition: opacity 0.15s;
    }

    .btn-login:hover { opacity: 0.9; }
    .btn-login:disabled { opacity: 0.5; cursor: not-allowed; }

    .error {
        color: #e53e3e;
        font-size: 0.8125rem;
        margin-top: 0.75rem;
    }
</style>

<div class="login-card">
    <div class="logo">
        <div class="logo-icon">CR</div>
        <span class="logo-text">CentuRisk</span>
    </div>
    <p class="subtitle">Select a user to log in as. Phase 1 demo — no password required.</p>
    <label for="user-select">Log in as</label>
    <select id="user-select">
        <option value="">Loading users...</option>
    </select>
    <div class="user-detail" id="user-detail">Select a user above.</div>
    <button class="btn-login" id="login-btn" disabled>Log In</button>
    <div class="error" id="error" style="display:none;"></div>
</div>
`;

const ROLE_CLASS = {
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

const ROLE_LABEL = {
    CentuRiskAdmin: "CentuRisk Admin",
    CentuRiskAnalyst: "CentuRisk Analyst",
    CentuRiskAuditor: "CentuRisk Auditor",
    CentuRiskSupport: "CentuRisk Support",
    PoolAdministrator: "Pool Administrator",
    PoolAnalyst: "Pool Analyst",
    PoolReadOnly: "Pool Read-Only",
    MemberAdmin: "Member Admin",
    MemberUser: "Member User",
    MemberReadOnly: "Member Read-Only",
};

class CenturiskLogin extends HTMLElement {
    constructor() {
        super();
        this.attachShadow({ mode: "open" });
        this.shadowRoot.appendChild(template.content.cloneNode(true));
        this._users = [];
    }

    connectedCallback() {
        this.shadowRoot.getElementById("user-select").addEventListener("change", () => this._onSelect());
        this.shadowRoot.getElementById("login-btn").addEventListener("click", () => this._login());
        this._loadUsers();
    }

    async _loadUsers() {
        try {
            const resp = await fetch("/api/users");
            this._users = await resp.json();
        } catch (e) {
            this._users = [];
        }

        const select = this.shadowRoot.getElementById("user-select");
        select.innerHTML = '<option value="">Choose a user...</option>';
        for (const u of this._users) {
            const opt = document.createElement("option");
            opt.value = u.user_id;
            const role = ROLE_LABEL[u.category] || u.category;
            opt.textContent = u.display_name + " (" + role + ")";
            select.appendChild(opt);
        }
    }

    _onSelect() {
        const userId = this.shadowRoot.getElementById("user-select").value;
        const detail = this.shadowRoot.getElementById("user-detail");
        const btn = this.shadowRoot.getElementById("login-btn");

        if (!userId) {
            detail.innerHTML = "Select a user above.";
            btn.disabled = true;
            return;
        }

        const user = this._users.find(u => u.user_id === userId);
        if (!user) return;

        const roleClass = ROLE_CLASS[user.category] || "role-member";
        const roleLabel = ROLE_LABEL[user.category] || user.category;
        let info = '<span class="role-badge ' + roleClass + '">' + roleLabel + '</span><br>';
        if (user.pool_id) info += "Pool: " + user.pool_id.substring(0, 8) + "...<br>";
        if (user.member_id) info += "Member: " + user.member_id.substring(0, 8) + "...";
        if (!user.pool_id && !user.member_id) info += "Cross-pool access";

        detail.innerHTML = info;
        btn.disabled = false;
    }

    async _login() {
        const userId = this.shadowRoot.getElementById("user-select").value;
        const errorEl = this.shadowRoot.getElementById("error");
        errorEl.style.display = "none";

        try {
            const resp = await fetch("/api/login", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ user_id: userId }),
            });

            if (!resp.ok) throw new Error("Login failed");

            const data = await resp.json();

            // Store token in cookie and localStorage
            document.cookie = "centurisk_session=" + data.token + ";path=/;SameSite=Strict";
            localStorage.setItem("centurisk_token", data.token);
            localStorage.setItem("centurisk_user", JSON.stringify(data.user));

            // Reload the app
            window.location.reload();
        } catch (e) {
            errorEl.textContent = e.message;
            errorEl.style.display = "block";
        }
    }
}

customElements.define("centurisk-login", CenturiskLogin);
