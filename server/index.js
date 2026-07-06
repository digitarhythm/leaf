const express = require('express');
const axios = require('axios');
const cors = require('cors');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');

const app = express();
app.use(cors());
app.use(express.json());

// .env の場所を探索
const envPaths = [
    path.join(__dirname, '../.env'),      // /leaf/.env (今回指定の場所)
    path.join(__dirname, '../../.env'),   // /var/wwws/digitarhythm.net/.env
    path.join(__dirname, '.env')          // server/.env
];

let envLoaded = false;
for (const envPath of envPaths) {
    if (fs.existsSync(envPath)) {
        console.log(`[Backend] Found .env at: ${envPath}`);
        require('dotenv').config({ path: envPath });
        envLoaded = true;
        break;
    }
}

const PORT = process.env.PORT || 3000;
const CLIENT_ID = process.env.LEAF_CLIENTID;
const CLIENT_SECRET = process.env.LEAF_CLIENT_SECRET;
// Google Identity Services のポップアップモードでは 'postmessage' を指定する必要がある
const REDIRECT_URI = 'postmessage';

// ---- マルチアプリ共用: apps.json 対応表 ----
// app_id -> { client_id, client_secret, shared_key } のマルチテナント対応表。
// secret を含むため .gitignore 済み。.env と同様に複数候補パスを探索して読み込む。
const appsJsonPaths = [
    path.join(__dirname, '../apps.json'),   // /leaf/apps.json
    path.join(__dirname, '../../apps.json'),// /var/wwws/digitarhythm.net/apps.json
    path.join(__dirname, 'apps.json')       // server/apps.json
];

let APPS = {};
for (const p of appsJsonPaths) {
    if (fs.existsSync(p)) {
        try {
            APPS = JSON.parse(fs.readFileSync(p, 'utf8'));
            console.log(`[Backend] Found apps.json at: ${p} (registered apps: ${Object.keys(APPS).join(', ') || 'none'})`);
        } catch (e) {
            console.error(`[Backend] Failed to parse apps.json at ${p}:`, e.message);
        }
        break;
    }
}

console.log('[Backend] Starting with configuration:');
console.log(`  PORT: ${PORT}`);
console.log(`  CLIENT_ID: ${CLIENT_ID ? 'OK' : 'MISSING'}`);
console.log(`  CLIENT_SECRET: ${CLIENT_SECRET ? 'OK' : 'MISSING'}`);
console.log(`  REDIRECT_URI: ${REDIRECT_URI}`);
console.log(`  Multi-app registry: ${Object.keys(APPS).length} app(s)`);

if (!CLIENT_ID || !CLIENT_SECRET) {
    console.error('[Backend] FATAL ERROR: Missing required environment variables. Check your .env file.');
}

// タイミング安全な文字列比較（共有キー検証用）
function safeEqual(a, b) {
    if (typeof a !== 'string' || typeof b !== 'string') return false;
    const ba = Buffer.from(a);
    const bb = Buffer.from(b);
    if (ba.length !== bb.length) return false;
    return crypto.timingSafeEqual(ba, bb);
}

// リクエストヘッダから使用するクレデンシャルを決定する。
// - X-App-Id が無い場合: レガシー互換（.env の LEAF_CLIENTID/SECRET、キー検証なし）
// - X-App-Id が有る場合: apps.json を参照し、X-App-Key を必須検証
// 戻り値: { ok: true, client_id, client_secret } または { ok: false, status, error }
function resolveCredentials(req) {
    const appId = req.get('X-App-Id');

    // レガシー互換モード
    if (!appId) {
        if (!CLIENT_SECRET) return { ok: false, status: 500, error: 'Server not configured (Secret missing)' };
        return { ok: true, client_id: CLIENT_ID, client_secret: CLIENT_SECRET };
    }

    // マルチテナントモード
    const entry = APPS[appId];
    if (!entry) {
        return { ok: false, status: 404, error: `Unknown app_id: ${appId}` };
    }
    const appKey = req.get('X-App-Key');
    if (!appKey || !safeEqual(appKey, entry.shared_key || '')) {
        return { ok: false, status: 401, error: 'Invalid or missing app key' };
    }
    if (!entry.client_id || !entry.client_secret) {
        return { ok: false, status: 500, error: `App '${appId}' is misconfigured (client_id/secret missing)` };
    }
    return { ok: true, client_id: entry.client_id, client_secret: entry.client_secret };
}

// 認可コードをトークンに交換
app.post('/api/auth/token', async (req, res) => {
    const { code, redirect_uri } = req.body;
    console.log(`[Backend] Token exchange request received (app_id: ${req.get('X-App-Id') || 'legacy'})`);

    if (!code) return res.status(400).json({ error: 'Code is required' });

    const cred = resolveCredentials(req);
    if (!cred.ok) return res.status(cred.status).json({ error: cred.error });

    // Tauri などのネイティブクライアントからはカスタムリダイレクトURIが送られてくる
    const finalRedirectUri = redirect_uri || REDIRECT_URI;
    console.log(`[Backend] Using redirect_uri: ${finalRedirectUri}`);

    try {
        const response = await axios.post('https://oauth2.googleapis.com/token', {
            code,
            client_id: cred.client_id,
            client_secret: cred.client_secret,
            redirect_uri: finalRedirectUri,
            grant_type: 'authorization_code',
        });
        res.json(response.data);
    } catch (error) {
        console.error('[Backend] Exchange failed:', error.response?.data || error.message);
        res.status(500).json({ error: 'Token exchange failed', details: error.response?.data });
    }
});

// リフレッシュトークンで更新
app.post('/api/auth/refresh', async (req, res) => {
    const { refresh_token } = req.body;
    if (!refresh_token) return res.status(400).json({ error: 'Refresh token is required' });

    const cred = resolveCredentials(req);
    if (!cred.ok) return res.status(cred.status).json({ error: cred.error });

    try {
        const response = await axios.post('https://oauth2.googleapis.com/token', {
            refresh_token,
            client_id: cred.client_id,
            client_secret: cred.client_secret,
            grant_type: 'refresh_token',
        });
        res.json(response.data);
    } catch (error) {
        console.error('[Backend] Refresh failed:', error.response?.data || error.message);
        res.status(500).json({ error: 'Refresh failed', details: error.response?.data });
    }
});

app.listen(PORT, '0.0.0.0', () => {
    console.log(`[Leaf-Backend] Auth proxy running on http://localhost:${PORT}`);
});
