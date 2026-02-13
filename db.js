// db.js
// IndexedDB wrapper

const STORE_SHEETS = 'sheets';
const STORE_SETTINGS = 'settings';
const STORE_CATEGORIES = 'categories';

let db;

export function init_db(dbName) {
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

export function save_sheet(sheet) {
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

export function load_sheets() {
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

export function delete_sheet(id) {
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

export function save_categories(categories) {
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

export function load_categories() {
    return new Promise((resolve, reject) => {
        if (!db) { reject("DB not initialized"); return; }
        const transaction = db.transaction([STORE_CATEGORIES], "readonly");
        const store = transaction.objectStore(STORE_CATEGORIES);
        const request = store.getAll();
        request.onsuccess = () => resolve(request.result);
        request.onerror = (e) => reject(e.target.error);
    });
}
