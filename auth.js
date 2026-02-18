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
            scope: 'openid email https://www.googleapis.com/auth/drive.file',
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
                
                // Rust 側へ通知
                window.dispatchEvent(new CustomEvent('leaf-token-refreshed', { detail: accessToken }));

                if (refreshPromise) {
                    refreshPromise.resolve(accessToken);
                    refreshPromise = null;
                }

                if (onSuccessCallback) {
                    onSuccessCallback(accessToken);
                }
            },
        });

        // 定期的にトークン期限をチェックして、期限が近ければリフレッシュするタイマー
        setInterval(async () => {
            const expiry = localStorage.getItem(EXPIRY_KEY);
            if (expiry) {
                const timeLeft = parseInt(expiry) - Date.now();
                // 残り10分を切っていたらリフレッシュ
                if (timeLeft < 10 * 60 * 1000) {
                    console.log("[Auth] Token nearing expiry (" + Math.round(timeLeft/1000/60) + " min left). Proactive refresh starting...");
                    try {
                        await try_silent_refresh();
                    } catch (e) {
                        console.warn("[Auth] Proactive refresh failed. Google session might be expired.");
                    }
                }
            }
        }, 60 * 1000); // 1分ごとにチェック
        
        const existingToken = localStorage.getItem(STORAGE_KEY);
        const expiry = localStorage.getItem(EXPIRY_KEY);
        
        if (existingToken && expiry && parseInt(expiry) > Date.now()) {
            accessToken = existingToken;
            console.log("[Auth] Existing valid token found.");
            if (onSuccessCallback) onSuccessCallback(accessToken);
        } else {
            console.log("[Auth] No valid session found. Waiting for user to sign in manually.");
        }
    };
    document.body.appendChild(script);
}

export async function try_silent_refresh() {
    if (!tokenClient) return null;
    
    // 既に実行中ならその完了を待つ
    if (refreshPromise) return refreshPromise.promise;

    console.log("[Auth] Attempting silent refresh (prompt: none)...");
    let res, rej;
    const promise = new Promise((resolve, reject) => {
        res = resolve;
        rej = reject;
    });
    refreshPromise = { promise, resolve: res, reject: rej };

    try {
        // prompt: 'none' を指定してユーザー操作なしでの更新を試みる
        tokenClient.requestAccessToken({ prompt: 'none' });
        return await promise;
    } catch (e) {
        console.error("[Auth] Silent refresh request failed:", e);
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

export async function get_access_token() {
    const expiry = localStorage.getItem(EXPIRY_KEY);
    if (expiry && parseInt(expiry) < Date.now()) {
        console.warn("[Auth] Token expired. Attempting silent refresh...");
        try {
            return await try_silent_refresh();
        } catch (e) {
            console.error("[Auth] Background refresh failed:", e);
            return null;
        }
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
    // アプリ側に通知
    window.dispatchEvent(new CustomEvent('leaf-auth-expired'));
}
