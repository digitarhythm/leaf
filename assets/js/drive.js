// drive.js
// Google Drive API wrapper with automatic token refresh and retry logic

import { get_access_token, try_silent_refresh, sign_out, force_reauth } from './auth.js';

export const FOLDER_MIME_TYPE = 'application/vnd.google-apps.folder';
const FILE_MIME_TYPE = 'text/plain';

// === appDataFolder 移行対応 ===
// appDataFolder（アプリ専用・非表示領域）の特殊エイリアス
const APPDATA_ROOT = 'appDataFolder';
// 移行完了マーカーファイル名
const MIGRATION_MARKER = '.leaf_migration_done';
// true の間、一覧/検索クエリに spaces=appDataFolder を付与する。
// ensure_directory_structure() が起動時に確定させる。
let APPDATA_MODE = false;

// 一覧/検索クエリに付与する spaces パラメータ（appDataFolder モード時のみ）
function spacesParam() {
    return APPDATA_MODE ? '&spaces=appDataFolder' : '';
}

/**
 * 指数バックオフによる待機
 */
const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));

/**
 * 認証付きフェッチ。401エラー時のリフレッシュや、ネットワークエラー時のリトライをサポート。
 */
async function authenticatedFetch(url, options = {}, retryCount = 2) {
    const token = await get_access_token();
    if (!token) {
        throw new Error("UNAUTHORIZED");
    }

    // すでに中断されている場合はリクエストしない
    if (options.signal && options.signal.aborted) {
        throw new Error("AbortError");
    }

    const headers = {
        'Authorization': `Bearer ${token}`,
        ...options.headers
    };

    try {
        const response = await fetch(url, { ...options, headers });

        if (response.status === 401 && retryCount > 0) {
            console.warn("[Drive] 401 Unauthorized. Attempting refresh...");
            try {
                await try_silent_refresh();
                return await authenticatedFetch(url, options, retryCount - 1);
            } catch (e) {
                console.warn("[Drive] Silent refresh failed. Triggering popup re-auth...");
                try {
                    await force_reauth();
                    return await authenticatedFetch(url, options, retryCount - 1);
                } catch (reauthError) {
                    sign_out();
                    throw new Error("UNAUTHORIZED");
                }
            }
        }

        return response;
    } catch (e) {
        if (e.name === 'AbortError') throw e;

        // ネットワークエラー時のみリトライ（指数バックオフ）
        if (retryCount > 0) {
            const waitTime = (3 - retryCount) * 1000;
            console.warn(`[Drive] Network error. Retrying in ${waitTime}ms...`, e);
            await sleep(waitTime);
            return await authenticatedFetch(url, options, retryCount - 1);
        }

        console.error("[Drive] Fetch failed after retries:", e);
        throw new Error("NETWORK_ERROR");
    }
}

export async function list_folders(parentId = 'root') {
    const query = `'${parentId}' in parents and mimeType = '${FOLDER_MIME_TYPE}' and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)${spacesParam()}`;
    const response = await authenticatedFetch(url);
    if (!response.ok) throw new Error(`List folders failed: ${response.status}`);
    return await response.json();
}

export async function create_folder(folderName, parentId) {
    const createRes = await authenticatedFetch('https://www.googleapis.com/drive/v3/files', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
            name: folderName,
            mimeType: FOLDER_MIME_TYPE,
            parents: [parentId]
        })
    });

    if (!createRes.ok) throw new Error(`Create folder failed: ${createRes.status}`);
    const folderData = await createRes.json();
    return folderData.id;
}

export async function find_or_create_folder(folderName, parentId = 'root') {
    const query = `mimeType='${FOLDER_MIME_TYPE}' and name='${folderName}' and '${parentId}' in parents and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)${spacesParam()}`;
    const searchRes = await authenticatedFetch(url);
    
    if (!searchRes.ok) throw new Error(`Search folder failed: ${searchRes.status}`);
    const searchData = await searchRes.json();
    
    if (searchData.files && searchData.files.length > 0) {
        return searchData.files[0].id;
    }
    
    const createRes = await authenticatedFetch('https://www.googleapis.com/drive/v3/files', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
            name: folderName,
            mimeType: FOLDER_MIME_TYPE,
            parents: [parentId]
        })
    });

    if (!createRes.ok) throw new Error(`Create folder failed: ${createRes.status}`);
    const folderData = await createRes.json();
    return folderData.id;
}

// === appDataFolder 移行ヘルパー群 ===

// appDataFolder 領域内でファイル/フォルダを名前検索する
async function find_in_appdata(name, parentId = APPDATA_ROOT) {
    const query = `name='${name.replace(/'/g, "\\'")}' and '${parentId}' in parents and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)&spaces=appDataFolder`;
    const res = await authenticatedFetch(url);
    if (res.status === 403) {
        // appdata スコープ未付与（既存ユーザーの旧トークン）
        const body = await res.text().catch(() => '');
        if (/scope|insufficient/i.test(body)) throw new Error('INSUFFICIENT_SCOPE');
        throw new Error('Find in appDataFolder failed: 403');
    }
    if (!res.ok) throw new Error(`Find in appDataFolder failed: ${res.status}`);
    const data = await res.json();
    return data.files && data.files.length > 0 ? data.files[0] : null;
}

// 指定領域のサブフォルダ一覧を取得する（useAppData で領域を切替）
async function list_child_folders(parentId, useAppData) {
    const query = `'${parentId}' in parents and mimeType = '${FOLDER_MIME_TYPE}' and trashed=false`;
    let url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`;
    if (useAppData) url += '&spaces=appDataFolder';
    const res = await authenticatedFetch(url);
    if (!res.ok) throw new Error(`List child folders failed: ${res.status}`);
    const data = await res.json();
    return data.files || [];
}

// 指定領域のサブファイル一覧を取得する（useAppData で領域を切替）
async function list_child_files(parentId, useAppData) {
    const query = `'${parentId}' in parents and mimeType != '${FOLDER_MIME_TYPE}' and trashed=false`;
    let url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`;
    if (useAppData) url += '&spaces=appDataFolder';
    const res = await authenticatedFetch(url);
    if (!res.ok) throw new Error(`List child files failed: ${res.status}`);
    const data = await res.json();
    return data.files || [];
}

// appDataFolder 領域内にフォルダを新規作成する
async function create_in_appdata_folder(name, parentId) {
    const res = await authenticatedFetch('https://www.googleapis.com/drive/v3/files', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name, mimeType: FOLDER_MIME_TYPE, parents: [parentId] })
    });
    if (!res.ok) throw new Error(`Create appDataFolder folder failed: ${res.status}`);
    return (await res.json()).id;
}

// 既存バイト列（BOM込み）をそのまま appDataFolder 領域へ新規アップロードする。
// 移行時は元ファイルの内容をそのまま複製するため、追加のBOMは付与しない。
async function upload_raw_to_appdata(filename, rawBytes, parentId) {
    const boundary = '-------314159265358979323846';
    const encoder = new TextEncoder();
    const metadata = { name: filename, mimeType: FILE_MIME_TYPE, parents: [parentId] };
    const part1 = `--${boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n${JSON.stringify(metadata)}\r\n`;
    const part2 = `--${boundary}\r\nContent-Type: ${FILE_MIME_TYPE}\r\n\r\n`;
    const end = `\r\n--${boundary}--`;
    const body = new Blob([encoder.encode(part1), encoder.encode(part2), rawBytes, encoder.encode(end)],
                          { type: `multipart/related; boundary=${boundary}` });
    const res = await authenticatedFetch('https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,name', {
        method: 'POST',
        body
    });
    if (!res.ok) throw new Error(`Upload to appDataFolder failed: ${res.status}`);
    return await res.json();
}

// マイドライブ側の旧 LeafData フォルダIDを探す（無ければ null）。作成はしない。
async function find_old_leafdata() {
    const findFolder = async (name, parentId) => {
        const query = `mimeType='${FOLDER_MIME_TYPE}' and name='${name}' and '${parentId}' in parents and trashed=false`;
        const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`;
        const res = await authenticatedFetch(url);
        if (!res.ok) throw new Error(`Find old folder failed: ${res.status}`);
        const data = await res.json();
        return data.files && data.files.length > 0 ? data.files[0].id : null;
    };
    const appSupportId = await findFolder('ApplicationSupport', 'root');
    if (!appSupportId) return null;
    return await findFolder('LeafData', appSupportId);
}

// appDataFolder 内で同名フォルダを探し、無ければ作成する（冪等）
async function find_or_create_appdata_folder(name, parentId) {
    const query = `mimeType='${FOLDER_MIME_TYPE}' and name='${name.replace(/'/g, "\\'")}' and '${parentId}' in parents and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)&spaces=appDataFolder`;
    const res = await authenticatedFetch(url);
    if (!res.ok) throw new Error(`Find appDataFolder folder failed: ${res.status}`);
    const data = await res.json();
    if (data.files && data.files.length > 0) return data.files[0].id;
    return await create_in_appdata_folder(name, parentId);
}

// appDataFolder 内に同名ファイルがあれば内容を上書き更新、無ければ新規作成する（冪等）。
// 中断→再開時の重複コピーを防ぐ。
async function upsert_raw_to_appdata(filename, rawBytes, parentId) {
    const query = `name='${filename.replace(/'/g, "\\'")}' and '${parentId}' in parents and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)&spaces=appDataFolder`;
    const res = await authenticatedFetch(url);
    if (!res.ok) throw new Error(`Find appDataFolder file failed: ${res.status}`);
    const data = await res.json();
    if (data.files && data.files.length > 0) {
        // 既存 → メディア更新（重複作成を防止）
        const fileId = data.files[0].id;
        const updateUrl = `https://www.googleapis.com/upload/drive/v3/files/${fileId}?uploadType=media&fields=id,name`;
        const upRes = await authenticatedFetch(updateUrl, {
            method: 'PATCH',
            headers: { 'Content-Type': FILE_MIME_TYPE },
            body: new Blob([rawBytes], { type: FILE_MIME_TYPE })
        });
        if (!upRes.ok) throw new Error(`Update appDataFolder file failed: ${upRes.status}`);
        return await upRes.json();
    }
    return await upload_raw_to_appdata(filename, rawBytes, parentId);
}

// 旧 LeafData 配下（カテゴリー/ファイル）を appDataFolder 直下へ複製する。
// 冪等に実装しており、中断後に再実行しても重複しない。
async function migrate_old_to_appdata(oldLeafDataId) {
    // カテゴリー（旧 LeafData 直下のフォルダ）ごとに複製
    const categories = await list_child_folders(oldLeafDataId, false);
    for (const cat of categories) {
        const newCatId = await find_or_create_appdata_folder(cat.name, APPDATA_ROOT);
        const files = await list_child_files(cat.id, false);
        for (const f of files) {
            const bytes = await download_file(f.id); // ID指定・BOM込みで取得
            await upsert_raw_to_appdata(f.name, bytes, newCatId);
        }
    }
    // 旧 LeafData 直下に直接置かれたファイル（通常は無いが念のため）
    const looseFiles = await list_child_files(oldLeafDataId, false);
    for (const f of looseFiles) {
        const bytes = await download_file(f.id);
        await upsert_raw_to_appdata(f.name, bytes, APPDATA_ROOT);
    }
}

// 移行完了マーカーを appDataFolder 直下に作成する（全処理の最後に呼ぶ）
async function create_migration_marker() {
    const existing = await find_in_appdata(MIGRATION_MARKER, APPDATA_ROOT);
    if (existing) return existing;
    const boundary = '-------314159265358979323846';
    const encoder = new TextEncoder();
    const metadata = { name: MIGRATION_MARKER, mimeType: FILE_MIME_TYPE, parents: [APPDATA_ROOT] };
    const part1 = `--${boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n${JSON.stringify(metadata)}\r\n`;
    const part2 = `--${boundary}\r\nContent-Type: ${FILE_MIME_TYPE}\r\n\r\n`;
    const end = `\r\n--${boundary}--`;
    const body = new Blob([encoder.encode(part1), encoder.encode(part2), encoder.encode(''), encoder.encode(end)],
                          { type: `multipart/related; boundary=${boundary}` });
    const res = await authenticatedFetch('https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id', {
        method: 'POST',
        body
    });
    if (!res.ok) throw new Error(`Create migration marker failed: ${res.status}`);
    return await res.json();
}

// 起動時ディレクトリ構造の確定（appDataFolder ベース）。
// 旧マイドライブ領域からの一度きりの移行もここで行う。
async function ensure_directory_structure_impl() {
    // 1. 移行完了マーカーの有無を確認
    APPDATA_MODE = true;
    const marker = await find_in_appdata(MIGRATION_MARKER, APPDATA_ROOT);
    if (marker) {
        // 移行済み → appDataFolder をそのまま利用
        const othersId = await find_or_create_folder('OTHERS', APPDATA_ROOT);
        return { appSupportId: null, leafDataId: APPDATA_ROOT, othersId };
    }

    // 2. 未移行 → 旧マイドライブ領域を確認
    APPDATA_MODE = false; // 旧領域（通常spaces）を検索するため一時的に無効化
    const oldLeafDataId = await find_old_leafdata();
    if (oldLeafDataId) {
        // 既存ユーザー：旧データを appDataFolder へ複製し、旧フォルダをリネームして保全
        // 移行開始をUIへ通知（「データのコンバート中」表示に切替）
        try { window.dispatchEvent(new CustomEvent('leaf-migration-start')); } catch (e) {}
        await migrate_old_to_appdata(oldLeafDataId);
        try {
            await rename_folder(oldLeafDataId, 'LeafData_backup');
        } catch (e) {
            console.warn('[Drive] 旧フォルダのリネームに失敗（移行自体は完了）:', e);
        }
        // 移行完了をUIへ通知
        try { window.dispatchEvent(new CustomEvent('leaf-migration-end')); } catch (e) {}
    }

    // 3. appDataFolder 側の初期化とマーカー作成
    APPDATA_MODE = true;
    const othersId = await find_or_create_folder('OTHERS', APPDATA_ROOT);
    await create_migration_marker();
    return { appSupportId: null, leafDataId: APPDATA_ROOT, othersId };
}

export async function ensure_directory_structure() {
    try {
        return await ensure_directory_structure_impl();
    } catch (e) {
        // appdata スコープ未付与の既存ユーザーは、再同意を促してから再試行
        if (String(e && e.message || e).includes('INSUFFICIENT_SCOPE')) {
            console.warn('[Drive] appDataFolder スコープ未付与。再認証を要求します。');
            await force_reauth();
            return await ensure_directory_structure_impl();
        }
        console.error("[Drive] Directory structure setup failed:", e);
        throw e;
    }
}

function buildMultipartBody(filename, content, folderId, boundary) {
    const encoder = new TextEncoder();
    const metadata = { name: filename, mimeType: FILE_MIME_TYPE };
    if (folderId) metadata.parents = [folderId];

    const part1 = `--${boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n${JSON.stringify(metadata)}\r\n`;
    const part2 = `--${boundary}\r\nContent-Type: ${FILE_MIME_TYPE}\r\n\r\n`;
    const end = `\r\n--${boundary}--`;

    // BOMは付与しない（BOM強制は廃止）
    return new Blob([encoder.encode(part1), encoder.encode(part2), content, encoder.encode(end)],
                    { type: `multipart/related; boundary=${boundary}` });
}

export async function move_file(fileId, oldParentId, newParentId) {
    const url = `https://www.googleapis.com/drive/v3/files/${fileId}?addParents=${newParentId}&removeParents=${oldParentId}&fields=id,parents`;
    const response = await authenticatedFetch(url, { method: 'PATCH' });
    if (!response.ok) throw new Error(`Move failed: ${response.status}`);
    return await response.json();
}

export async function rename_folder(folderId, newName) {
    const url = `https://www.googleapis.com/drive/v3/files/${folderId}`;
    const response = await authenticatedFetch(url, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: newName })
    });
    if (!response.ok) throw new Error(`Rename folder failed: ${response.status}`);
    return await response.json();
}

export async function rename_file(fileId, newName) {
    const url = `https://www.googleapis.com/drive/v3/files/${fileId}`;
    const response = await authenticatedFetch(url, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: newName })
    });
    if (!response.ok) throw new Error(`Rename file failed: ${response.status}`);
    return await response.json();
}

export async function upload_file(filename, content, folderId, fileId = null) {
    // BOMは付与しない（BOM強制は廃止）
    const contentBlob = new Blob([content], { type: FILE_MIME_TYPE });

    if (fileId) {
        const url = `https://www.googleapis.com/upload/drive/v3/files/${fileId}?uploadType=media&fields=id,name,modifiedTime`;
        const response = await authenticatedFetch(url, {
            method: 'PATCH',
            headers: { 'Content-Type': FILE_MIME_TYPE },
            body: contentBlob
        });

        if (response.ok) return await response.json();
        if (response.status !== 404) throw new Error(`Upload failed: ${response.status}`);
    }

    const boundary = '-------314159265358979323846';
    const body = buildMultipartBody(filename, content, folderId, boundary);
    const response = await authenticatedFetch(`https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,name,modifiedTime`, {
        method: 'POST',
        body: body
    });

    if (!response.ok) throw new Error(`Upload failed: ${response.status}`);
    return await response.json();
}

export async function list_files(folderId, signal = null) {
    const query = `'${folderId}' in parents and mimeType != '${FOLDER_MIME_TYPE}' and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name, size, modifiedTime, createdTime)${spacesParam()}`;
    const response = await authenticatedFetch(url, { signal });
    if (!response.ok) throw new Error(`List files failed: ${response.status}`);
    return await response.json();
}

export async function delete_file(fileId) {
    const response = await authenticatedFetch(`https://www.googleapis.com/drive/v3/files/${fileId}`, { method: 'DELETE' });
    if (!response.ok && response.status !== 404) throw new Error(`Delete failed: ${response.status}`);
    return true;
}

export async function find_file_by_name(filename, folderId) {
    const query = `name='${filename.replace(/'/g, "\\'")}' and '${folderId}' in parents and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)${spacesParam()}`;
    const response = await authenticatedFetch(url);
    if (!response.ok) throw new Error(`Find file failed: ${response.status}`);
    const data = await response.json();
    return data.files && data.files.length > 0 ? data.files[0] : null;
}

export function parse_date(dateStr) {
    return Date.parse(dateStr);
}

export async function download_file(fileId, range = null, signal = null) {
    try {
        const url = `https://www.googleapis.com/drive/v3/files/${fileId}?alt=media`;
        const options = { signal };
        if (range) options.headers = { 'Range': `bytes=${range}` };

        const response = await authenticatedFetch(url, options);
        
        if (response.status === 416) return new Uint8Array(0);
        
        if (!response.ok && response.status !== 206) {
            return new Uint8Array(0);
        }

        const buffer = await response.arrayBuffer();
        return new Uint8Array(buffer);
    } catch (e) {
        if (e.name === 'AbortError' || e.message === 'AbortError') return new Uint8Array(0);
        console.error(`[Drive] download_file error for ${fileId}:`, e);
        return new Uint8Array(0);
    }
}

export async function get_file_metadata(fileId) {
    const url = `https://www.googleapis.com/drive/v3/files/${fileId}?fields=id,name,size,modifiedTime,trashed,parents`;
    const response = await authenticatedFetch(url);
    if (!response.ok) throw new Error(`Get metadata failed: ${response.status}`);
    return await response.json();
}
