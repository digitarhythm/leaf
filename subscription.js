// subscription.js
// Stripe subscription status check with caching

import { is_tauri } from './editor_interop.js';
import { get_user_email } from './auth.js';

const CACHE_KEY = 'leaf_subscription_status';
const CACHE_EXPIRY_KEY = 'leaf_subscription_expiry';
const CACHE_TTL_MS = 30 * 60 * 1000; // 30 minutes

export async function check_subscription_status() {
    // Tauri desktop app: always treat as subscribed (no ads)
    if (is_tauri()) {
        return true;
    }

    // Check cache first
    const cached = get_cached_subscription_status();
    if (cached !== null) {
        return cached;
    }

    const email = get_user_email();
    if (!email) {
        console.warn("[Subscription] No email available. Assuming no subscription.");
        return false;
    }

    try {
        const res = await fetch(`/api/subscription/status?email=${encodeURIComponent(email)}`);
        if (!res.ok) throw new Error(`Subscription check failed: ${res.status}`);
        const data = await res.json();
        const status = !!data.has_subscription;

        // Cache result
        localStorage.setItem(CACHE_KEY, JSON.stringify(status));
        localStorage.setItem(CACHE_EXPIRY_KEY, (Date.now() + CACHE_TTL_MS).toString());
        console.log("[Subscription] Status:", status);
        return status;
    } catch (e) {
        console.warn("[Subscription] Check failed:", e);
        return false;
    }
}

export function get_cached_subscription_status() {
    const expiry = localStorage.getItem(CACHE_EXPIRY_KEY);
    if (!expiry || parseInt(expiry) < Date.now()) {
        return null; // Cache expired or not set
    }
    const cached = localStorage.getItem(CACHE_KEY);
    if (cached === null) return null;
    try {
        return JSON.parse(cached);
    } catch {
        return null;
    }
}

export function clear_subscription_cache() {
    localStorage.removeItem(CACHE_KEY);
    localStorage.removeItem(CACHE_EXPIRY_KEY);
    console.log("[Subscription] Cache cleared.");
}
