// db.js
// IndexedDB wrapper

const STORE_SHEETS = 'sheets';
const STORE_SETTINGS = 'settings';

let db;

export function init_db(dbName) {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open(dbName, 1);

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
        const request = store.put(sheet);

        request.onsuccess = () => resolve();
        request.onerror = (e) => reject(e.target.error);
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
