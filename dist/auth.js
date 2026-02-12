// auth.js
// Google Identity Services integration

let tokenClient;
let accessToken = null;
const STORAGE_KEY = 'leaf_google_access_token';

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
                    throw response;
                }
                accessToken = response.access_token;
                // Save session
                localStorage.setItem(STORAGE_KEY, accessToken);
                console.log("Access Token received and saved");
                if (onSuccessCallback) {
                    onSuccessCallback(accessToken);
                }
            },
        });
        
        // After initialization, check for existing session
        const existingToken = localStorage.getItem(STORAGE_KEY);
        if (existingToken) {
            console.log("Found existing session");
            accessToken = existingToken;
            if (onSuccessCallback) {
                onSuccessCallback(accessToken);
            }
        }
    };
    document.body.appendChild(script);
}

export function request_access_token() {
    if (tokenClient) {
        tokenClient.requestAccessToken({prompt: 'consent'});
    } else {
        console.error("Token client not initialized");
    }
}

export function get_access_token() {
    return accessToken;
}

export function is_signed_in() {
    return accessToken !== null;
}

export function sign_out() {
    accessToken = null;
    localStorage.removeItem(STORAGE_KEY);
    console.log("Signed out and session cleared");
}
