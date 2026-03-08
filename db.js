// db.js
// IndexedDB wrapper
// Google Identity Services integration - Code Model (Refresh Token support)
import { is_tauri } from './editor_interop.js';

// import Database from '@tauri-apps/plugin-sql'; 
// -> Trunk does not bundle bare ES module imports. We interact via invoke directly.
class TauriDatabase {
    constructor(path) {
        this.path = path;
    }
    static async load(path) {
        if (!window.__TAURI__) throw new Error("Tauri API not found");
        const _path = await window.__TAURI__.core.invoke('plugin:sql|load', { db: path });
        return new TauriDatabase(_path);
    }
    async execute(query, bindValues) {
        const [rowsAffected, lastInsertId] = await window.__TAURI__.core.invoke('plugin:sql|execute', {
            db: this.path,
            query,
            values: bindValues || []
        });
        return { lastInsertId, rowsAffected };
    }
    async select(query, bindValues) {
        return await window.__TAURI__.core.invoke('plugin:sql|select', {
            db: this.path,
            query,
            values: bindValues || []
        });
    }
}

const STORE_SHEETS = 'sheets';
const STORE_SETTINGS = 'settings';
const STORE_CATEGORIES = 'categories';

let db;
let tauriDb; // separate handle for SQLite

export async function init_db(dbName) {
    if (is_tauri()) {
        console.log("[DB-Tauri] Initializing native database (SQLite)");
        try {
            tauriDb = await TauriDatabase.load('sqlite:leaf.db');
            console.log("SQLite native database loaded.");
            return Promise.resolve();
        } catch (e) {
            console.error("Failed to load native SQLite db:", e);
            return Promise.reject(e);
        }
    }

    // アカウント別DB（LeafDB_xxx）を開く場合、古い「LeafDB」が残っていれば削除
    if (dbName.startsWith('LeafDB_')) {
        try {
            const databases = await indexedDB.databases();
            if (databases.some(d => d.name === 'LeafDB')) {
                indexedDB.deleteDatabase('LeafDB');
                console.log("[DB] Deleted legacy 'LeafDB' database");
            }
        } catch (_) {
            // indexedDB.databases() 非対応ブラウザではスキップ
        }
    }

    return new Promise((resolve, reject) => {
        // バージョンを2に上げる
        const request = indexedDB.open(dbName, 2);

        request.onerror = (event) => {
            console.error("IndexedDB error:", event.target.error);
            reject("IndexedDB error");
        };

        request.onsuccess = (event) => {
            db = event.target.result;
            console.log("IndexedDB initialized: " + dbName);
            resolve();
        };

        request.onupgradeneeded = (event) => {
            const db = event.target.result;
            if (!db.objectStoreNames.contains(STORE_SHEETS)) {
                db.createObjectStore(STORE_SHEETS, { keyPath: 'id' });
            }
            if (!db.objectStoreNames.contains(STORE_SETTINGS)) {
                db.createObjectStore(STORE_SETTINGS, { keyPath: 'key' });
            }
            if (!db.objectStoreNames.contains(STORE_CATEGORIES)) {
                db.createObjectStore(STORE_CATEGORIES, { keyPath: 'id' });
            }
        };
    });
}

export function close_db() {
    if (db) {
        db.close();
        db = null;
    }
}

export async function save_sheet(sheet) {
    if (is_tauri()) {
        console.log("[DB-Tauri] Saving sheet to native db: ", sheet.id);
        if (!tauriDb) return Promise.reject("Native DB not initialized");
        try {
            await tauriDb.execute(
                "INSERT OR REPLACE INTO sheets (id, title, content, updated_at, folder_id, is_trashed) VALUES ($1, $2, $3, $4, $5, $6)",
                [
                    sheet.id,
                    sheet.title,
                    sheet.content,
                    sheet.updated_at,
                    sheet.folder_id || null, // Ensure valid bind values
                    sheet.is_trashed ? 1 : 0
                ]
            );
            return Promise.resolve();
        } catch (e) {
            console.error("SQLite insert error:", e);
            return Promise.reject(e);
        }
    }

    return new Promise((resolve, reject) => {
        if (!db) {
            reject("DB not initialized");
            return;
        }
        const transaction = db.transaction([STORE_SHEETS], "readwrite");
        const store = transaction.objectStore(STORE_SHEETS);

        // 常に最新の1件のみを保持するため、既存データを全削除してから追加する
        const clearReq = store.clear();
        clearReq.onsuccess = () => {
            const request = store.put(sheet);
            request.onsuccess = () => resolve();
            request.onerror = (e) => reject(e.target.error);
        };
        clearReq.onerror = (e) => reject(e.target.error);
    });
}

export async function load_sheets() {
    if (is_tauri()) {
        console.log("[DB-Tauri] Loading sheets from native db (SQLite)");
        if (!tauriDb) return Promise.reject("Native DB not initialized");
        try {
            const rows = await tauriDb.select("SELECT * FROM sheets");
            // Normalize columns (SQLite returns numbers where IDB might expect boolean)
            const sheets = rows.map(row => ({
                ...row,
                is_trashed: row.is_trashed === 1
            }));
            return Promise.resolve(sheets);
        } catch (e) {
            console.error("SQLite select error:", e);
            return Promise.reject(e);
        }
    }

    return new Promise((resolve, reject) => {
        if (!db) {
            reject("DB not initialized");
            return;
        }
        const transaction = db.transaction([STORE_SHEETS], "readonly");
        const store = transaction.objectStore(STORE_SHEETS);
        const request = store.getAll();

        request.onsuccess = () => resolve(request.result);
        request.onerror = (e) => reject(e.target.error);
    });
}

export async function delete_sheet(id) {
    if (is_tauri()) {
        console.log("[DB-Tauri] Deleting sheet: ", id);
        if (!tauriDb) return Promise.reject("Native DB not initialized");
        try {
            await tauriDb.execute("DELETE FROM sheets WHERE id = $1", [id]);
            return Promise.resolve();
        } catch (e) {
            console.error("SQLite delete error:", e);
            return Promise.reject(e);
        }
    }

    return new Promise((resolve, reject) => {
        if (!db) {
            reject("DB not initialized");
            return;
        }
        const transaction = db.transaction([STORE_SHEETS], "readwrite");
        const store = transaction.objectStore(STORE_SHEETS);
        const request = store.delete(id);

        request.onsuccess = () => resolve();
        request.onerror = (e) => reject(e.target.error);
    });
}

export async function save_categories(categories) {
    if (is_tauri()) {
        console.log("[DB-Tauri] Saving categories");
        if (!tauriDb) return Promise.reject("Native DB not initialized");
        try {
            // Transaction-like approach for sync logic: clear and insert
            await tauriDb.execute("DELETE FROM categories");
            for (const cat of categories) {
                await tauriDb.execute(
                    "INSERT INTO categories (id, name, color, sort_order) VALUES ($1, $2, $3, $4)",
                    [cat.id, cat.name, cat.color || null, cat.sort_order || 0]
                );
            }
            return Promise.resolve();
        } catch (e) {
            console.error("SQLite list saving error:", e);
            return Promise.reject(e);
        }
    }

    return new Promise((resolve, reject) => {
        if (!db) { reject("DB not initialized"); return; }
        const transaction = db.transaction([STORE_CATEGORIES], "readwrite");
        const store = transaction.objectStore(STORE_CATEGORIES);

        // 既存のデータを全削除してから追加（常に最新状態に保つため）
        const clearReq = store.clear();
        clearReq.onsuccess = () => {
            for (const cat of categories) {
                store.add(cat);
            }
            resolve();
        };
        clearReq.onerror = (e) => reject(e.target.error);
    });
}

export async function load_categories() {
    if (is_tauri()) {
        console.log("[DB-Tauri] Loading categories (SQLite)");
        if (!tauriDb) return Promise.reject("Native DB not initialized");
        try {
            const categories = await tauriDb.select("SELECT * FROM categories ORDER BY sort_order ASC");
            return Promise.resolve(categories);
        } catch (e) {
            console.error("SQLite select error:", e);
            return Promise.reject(e);
        }
    }

    return new Promise((resolve, reject) => {
        if (!db) { reject("DB not initialized"); return; }
        const transaction = db.transaction([STORE_CATEGORIES], "readonly");
        const store = transaction.objectStore(STORE_CATEGORIES);
        const request = store.getAll();
        request.onsuccess = () => resolve(request.result);
        request.onerror = (e) => reject(e.target.error);
    });
}
