// adsense.js
// Google AdSense script loading and ad rendering

import { is_tauri } from './editor_interop.js';

const ADSENSE_PUB_ID = 'ca-pub-1064912999872599';
const ADSENSE_SLOT_ID = '1855516140';

let scriptLoaded = false;

export function load_adsense_script() {
    if (is_tauri()) {
        console.log("[AdSense] Tauri environment. Skipping AdSense script load.");
        return;
    }

    if (scriptLoaded) return;

    const script = document.createElement('script');
    script.async = true;
    script.src = `https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js?client=${ADSENSE_PUB_ID}`;
    script.crossOrigin = 'anonymous';
    script.onerror = () => {
        console.warn("[AdSense] Failed to load AdSense script (ad blocker?).");
    };
    document.head.appendChild(script);
    scriptLoaded = true;
    console.log("[AdSense] Script injected.");
}

export function render_ad(containerId) {
    if (is_tauri()) return;

    const container = document.getElementById(containerId);
    if (!container) {
        console.warn("[AdSense] Container not found:", containerId);
        return;
    }

    // Clear existing content
    container.innerHTML = '';

    const ins = document.createElement('ins');
    ins.className = 'adsbygoogle';
    ins.style.cssText = 'display:block; text-align:center;';
    ins.setAttribute('data-ad-layout', 'in-article');
    ins.setAttribute('data-ad-format', 'fluid');
    ins.setAttribute('data-ad-client', ADSENSE_PUB_ID);
    ins.setAttribute('data-ad-slot', ADSENSE_SLOT_ID);
    container.appendChild(ins);

    try {
        (window.adsbygoogle = window.adsbygoogle || []).push({});
        console.log("[AdSense] Ad rendered in:", containerId);
    } catch (e) {
        console.warn("[AdSense] adsbygoogle.push failed:", e);
    }
}

export function remove_ad(containerId) {
    const container = document.getElementById(containerId);
    if (container) {
        container.innerHTML = '';
        console.log("[AdSense] Ad removed from:", containerId);
    }
}
