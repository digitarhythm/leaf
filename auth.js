// auth.js
// Google Identity Services integration - Code Model (Refresh Token support)

let codeClient;
let accessToken = null;
let refreshPromise = null;
const STORAGE_KEY = 'leaf_google_access_token';
const EXPIRY_KEY = 'leaf_google_token_expiry';
const REFRESH_TOKEN_KEY = 'leaf_google_refresh_token';

let reauthPromise = null;

async function exchangeCodeForToken(code) {
    console.log("[Auth] Exchanging code for tokens via backend...");
    try {
        const response = await fetch('/api/auth/token', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ code })
        });
        if (!response.ok) {
            const errorData = await response.json().catch(() => ({}));
            console.error("[Auth] Backend error details:", errorData);
            throw new Error(`Token exchange failed with status ${response.status}`);
        }
        const data = await response.json();
        saveSession(data);
        return data.access_token;
    } catch (e) {
        console.error("[Auth] Exchange error (possibly offline):", e);
        throw e;
    }
}

function saveSession(data) {
    accessToken = data.access_token;
    const expiresAt = Date.now() + (parseInt(data.expires_in) * 1000) - (5 * 60 * 1000);
    
    localStorage.setItem(STORAGE_KEY, accessToken);
    localStorage.setItem(EXPIRY_KEY, expiresAt.toString());
    if (data.refresh_token) {
        localStorage.setItem(REFRESH_TOKEN_KEY, data.refresh_token);
    }
    
    console.log("Access Token received. Expires at:", new Date(expiresAt));
    window.dispatchEvent(new CustomEvent('leaf-token-refreshed', { detail: accessToken }));
}

export function init_google_auth(clientId, onSuccessCallback) {
    const script = document.createElement('script');
    script.src = 'https://accounts.google.com/gsi/client';
    script.async = true;
    script.defer = true;
    script.onload = () => {
        codeClient = google.accounts.oauth2.initCodeClient({
            client_id: clientId,
            scope: 'openid email https://www.googleapis.com/auth/drive.file',
            ux_mode: 'popup',
            callback: async (response) => {
                if (response.error !== undefined) {
                    console.error("Auth Error:", response);
                    if (refreshPromise) { refreshPromise.reject(response); refreshPromise = null; }
                    if (reauthPromise) { reauthPromise.reject(response); reauthPromise = null; }
                    return;
                }

                try {
                    const token = await exchangeCodeForToken(response.code);
                    if (refreshPromise) { refreshPromise.resolve(token); refreshPromise = null; }
                    if (reauthPromise) { reauthPromise.resolve(token); reauthPromise = null; }
                    if (onSuccessCallback) onSuccessCallback(token);
                } catch (e) {
                    if (refreshPromise) { refreshPromise.reject(e); refreshPromise = null; }
                    if (reauthPromise) { reauthPromise.reject(e); reauthPromise = null; }
                }
            },
        });

        // 定期的にトークン期限をチェック
        setInterval(async () => {
            const expiry = localStorage.getItem(EXPIRY_KEY);
            if (expiry) {
                const timeLeft = parseInt(expiry) - Date.now();
                if (timeLeft < 10 * 60 * 1000) {
                    console.log("[Auth] Token nearing expiry. Refreshing...");
                    try {
                        await try_silent_refresh();
                    } catch (e) {
                        console.warn("[Auth] Proactive refresh failed.");
                    }
                }
            }
        }, 60 * 1000);
        
        const existingToken = localStorage.getItem(STORAGE_KEY);
        const expiry = localStorage.getItem(EXPIRY_KEY);
        
        if (existingToken && expiry && parseInt(expiry) > Date.now()) {
            accessToken = existingToken;
            console.log("[Auth] Existing valid token found.");
            if (onSuccessCallback) onSuccessCallback(accessToken);
        } else if (localStorage.getItem(REFRESH_TOKEN_KEY)) {
            console.log("[Auth] Found refresh token. Attempting silent refresh...");
            try_silent_refresh().then(token => {
                if (token && onSuccessCallback) onSuccessCallback(token);
            }).catch(() => {
                console.log("[Auth] Refresh token expired or invalid.");
            });
        }
    };
    document.body.appendChild(script);
}

export async function try_silent_refresh() {
    const refreshToken = localStorage.getItem(REFRESH_TOKEN_KEY);
    if (!refreshToken) {
        // リフレッシュトークンがない場合は以前のポップアップ方式へフォールバック
        return force_reauth();
    }
    
    if (refreshPromise) return refreshPromise.promise;

    console.log("[Auth] Attempting refresh using refresh_token...");
    let res, rej;
    const promise = new Promise((resolve, reject) => { res = resolve; rej = reject; });
    refreshPromise = { promise, resolve: res, reject: rej };

    try {
        const response = await fetch('/api/auth/refresh', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ refresh_token: refreshToken })
        });
        if (!response.ok) throw new Error("Refresh failed");
        const data = await response.json();
        saveSession(data);
        refreshPromise.resolve(data.access_token);
        refreshPromise = null;
        return data.access_token;
    } catch (e) {
        console.error("[Auth] Refresh token failed (possibly offline):", e);
        refreshPromise.reject(e);
        refreshPromise = null;
        // ネットワークエラーでなければ再ログインを促す
        if (navigator.onLine) {
            return force_reauth();
        }
        throw e;
    }
}

export function request_access_token() {
    if (codeClient) {
        codeClient.requestCode();
    } else {
        console.error("[Auth] codeClient not initialized!");
    }
}

export async function get_access_token() {
    const expiry = localStorage.getItem(EXPIRY_KEY);
    if (expiry && parseInt(expiry) < Date.now()) {
        return await try_silent_refresh();
    }
    return accessToken;
}

export function is_signed_in() {
    const expiry = localStorage.getItem(EXPIRY_KEY);
    return (accessToken !== null && expiry && parseInt(expiry) > Date.now()) || !!localStorage.getItem(REFRESH_TOKEN_KEY);
}

export async function sign_out() {
    accessToken = null;
    localStorage.removeItem(STORAGE_KEY);
    localStorage.removeItem(EXPIRY_KEY);
    localStorage.removeItem(REFRESH_TOKEN_KEY);
    console.log("Signed out and session cleared");

    if ('serviceWorker' in navigator) {
        try {
            const registrations = await navigator.serviceWorker.getRegistrations();
            for (let registration of registrations) { await registration.unregister(); }
        } catch (e) { console.warn("[Auth] SW unregister failed:", e); }
    }
    window.dispatchEvent(new CustomEvent('leaf-auth-expired'));
}

export async function force_reauth() {
    if (!codeClient) return Promise.reject("CodeClient not initialized");
    if (reauthPromise) return reauthPromise.promise;

    console.log("[Auth] Forcing re-authentication via popup...");
    let res, rej;
    const promise = new Promise((resolve, reject) => { res = resolve; rej = reject; });
    reauthPromise = { promise, resolve: res, reject: rej };

    try {
        codeClient.requestCode();
        return await promise;
    } catch (e) {
        console.error("[Auth] Re-auth popup failed:", e);
        reauthPromise = null;
        throw e;
    }
}
