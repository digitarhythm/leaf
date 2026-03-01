const express = require('express');
const axios = require('axios');
const cors = require('cors');
const path = require('path');
const fs = require('fs');

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
const STRIPE_SECRET_KEY = process.env.STRIPE_SECRET_KEY;
// Google Identity Services のポップアップモードでは 'postmessage' を指定する必要がある
const REDIRECT_URI = 'postmessage';

console.log('[Backend] Starting with configuration:');
console.log(`  PORT: ${PORT}`);
console.log(`  CLIENT_ID: ${CLIENT_ID ? 'OK' : 'MISSING'}`);
console.log(`  CLIENT_SECRET: ${CLIENT_SECRET ? 'OK' : 'MISSING'}`);
console.log(`  REDIRECT_URI: ${REDIRECT_URI}`);

if (!CLIENT_ID || !CLIENT_SECRET) {
    console.error('[Backend] FATAL ERROR: Missing required environment variables. Check your .env file.');
}

// 認可コードをトークンに交換
app.post('/api/auth/token', async (req, res) => {
    const { code, redirect_uri } = req.body;
    console.log(`[Backend] Token exchange request received`);

    if (!code) return res.status(400).json({ error: 'Code is required' });
    if (!CLIENT_SECRET) return res.status(500).json({ error: 'Server not configured (Secret missing)' });

    // Tauri などのネイティブクライアントからはカスタムリダイレクトURIが送られてくる
    const finalRedirectUri = redirect_uri || REDIRECT_URI;
    console.log(`[Backend] Using redirect_uri: ${finalRedirectUri}`);

    try {
        const response = await axios.post('https://oauth2.googleapis.com/token', {
            code,
            client_id: CLIENT_ID,
            client_secret: CLIENT_SECRET,
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

    try {
        const response = await axios.post('https://oauth2.googleapis.com/token', {
            refresh_token,
            client_id: CLIENT_ID,
            client_secret: CLIENT_SECRET,
            grant_type: 'refresh_token',
        });
        res.json(response.data);
    } catch (error) {
        console.error('[Backend] Refresh failed:', error.response?.data || error.message);
        res.status(500).json({ error: 'Refresh failed', details: error.response?.data });
    }
});

// サブスクリプション状態チェック
app.get('/api/subscription/status', async (req, res) => {
    const { email } = req.query;
    if (!email) return res.status(400).json({ error: 'email is required' });

    if (!STRIPE_SECRET_KEY || STRIPE_SECRET_KEY === 'sk_test_PLACEHOLDER') {
        console.warn('[Backend] Stripe not configured. Returning has_subscription=false.');
        return res.json({ has_subscription: false });
    }

    try {
        const stripe = require('stripe')(STRIPE_SECRET_KEY);
        const customers = await stripe.customers.list({ email, limit: 1 });
        if (customers.data.length === 0) {
            return res.json({ has_subscription: false });
        }
        const customer = customers.data[0];
        const subscriptions = await stripe.subscriptions.list({
            customer: customer.id,
            status: 'active',
            limit: 1,
        });
        res.json({ has_subscription: subscriptions.data.length > 0 });
    } catch (error) {
        console.error('[Backend] Stripe check failed:', error.message);
        res.status(500).json({ error: 'Subscription check failed' });
    }
});

app.listen(PORT, '0.0.0.0', () => {
    console.log(`[Leaf-Backend] Auth proxy running on http://localhost:${PORT}`);
});
