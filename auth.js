// auth.js
// Google Identity Services integration

let tokenClient;
let accessToken = null;
let refreshPromise = null;
const STORAGE_KEY = 'leaf_google_access_token';
const EXPIRY_KEY = 'leaf_google_token_expiry';

export function init_google_auth(clientId, onSuccessCallback) {
    const script = document.createElement('script');
    script.src = 'https://accounts.google.com/gsi/client';
    script.async = true;
    script.defer = true;
    script.onload = () => {
        tokenClient = google.accounts.oauth2.initTokenClient({
            client_id: clientId,
            scope: 'openid email https://www.googleapis.com/auth/drive',
            callback: (response) => {
                if (response.error !== undefined) {
                    console.error("Auth Error:", response);
                    if (refreshPromise) refreshPromise.reject(response);
                    return;
                }
                accessToken = response.access_token;
                const expiresAt = Date.now() + (parseInt(response.expires_in) * 1000) - (5 * 60 * 1000);
                
                localStorage.setItem(STORAGE_KEY, accessToken);
                localStorage.setItem(EXPIRY_KEY, expiresAt.toString());
                
                console.log("Access Token received. Expires at:", new Date(expiresAt));
                
                if (refreshPromise) {
                    refreshPromise.resolve(accessToken);
                    refreshPromise = null;
                }

                if (onSuccessCallback) {
                    onSuccessCallback(accessToken);
                }
            },
        });
        
        const existingToken = localStorage.getItem(STORAGE_KEY);
        const expiry = localStorage.getItem(EXPIRY_KEY);
        
        if (existingToken && expiry && parseInt(expiry) > Date.now()) {
            accessToken = existingToken;
            console.log("[Auth] Existing valid token found.");
            if (onSuccessCallback) onSuccessCallback(accessToken);
        } else {
            console.log("[Auth] Session expired or not found. Attempting initial silent refresh...");
            // 初期化時のサイレントリフレッシュ。失敗しても単にログイン画面が出るだけなので例外はキャッチする。
            try_silent_refresh().catch(e => {
                console.log("[Auth] Initial silent refresh unavailable. User must sign in manually.");
            });
        }
    };
    document.body.appendChild(script);
}

export async function try_silent_refresh() {
    if (!tokenClient) return null;
    
    // 既に実行中ならその完了を待つ
    if (refreshPromise) return refreshPromise.promise;

    console.log("[Auth] Attempting silent refresh...");
    let res, rej;
    const promise = new Promise((resolve, reject) => {
        res = resolve;
        rej = reject;
    });
    refreshPromise = { promise, resolve: res, reject: rej };

    try {
        tokenClient.requestAccessToken({ prompt: '' });
        return await promise;
    } catch (e) {
        console.error("[Auth] Silent refresh failed:", e);
        refreshPromise = null;
        throw e;
    }
}

export function request_access_token() {
    console.log("[Auth] request_access_token called. Checking tokenClient...");
    if (tokenClient) {
        console.log("[Auth] Triggering requestAccessToken popup...");
        tokenClient.requestAccessToken({ prompt: 'select_account' });
    } else {
        console.error("[Auth] tokenClient not initialized yet!");
    }
}

export function get_access_token() {
    const expiry = localStorage.getItem(EXPIRY_KEY);
    if (expiry && parseInt(expiry) < Date.now()) {
        console.warn("[Auth] Token expired. Need refresh.");
        // ここでは非同期で開始するが、戻り値は古いトークンのまま
        try_silent_refresh().catch(() => {});
    }
    return accessToken;
}

export function is_signed_in() {
    const expiry = localStorage.getItem(EXPIRY_KEY);
    return accessToken !== null && expiry && parseInt(expiry) > Date.now();
}

export function sign_out() {
    accessToken = null;
    localStorage.removeItem(STORAGE_KEY);
    localStorage.removeItem(EXPIRY_KEY);
    console.log("Signed out and session cleared");
}
