// drive.js
// Google Drive API wrapper

import { get_access_token, sign_out } from './auth.js';

export const FOLDER_MIME_TYPE = 'application/vnd.google-apps.folder';
const FILE_MIME_TYPE = 'text/plain';

async function getHeaders() {
    const token = get_access_token();
    if (!token) throw new Error("No access token");
    return {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json'
    };
}

export async function list_folders(parentId = 'root') {
    const headers = await getHeaders();
    const query = `'${parentId}' in parents and mimeType = '${FOLDER_MIME_TYPE}' and trashed=false`;
    const response = await fetch(`https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`, { headers });
    if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    return await response.json();
}

export async function create_folder(folderName, parentId) {
    const headers = await getHeaders();
    const createRes = await fetch('https://www.googleapis.com/drive/v3/files', {
        method: 'POST',
        headers,
        body: JSON.stringify({
            name: folderName,
            mimeType: FOLDER_MIME_TYPE,
            parents: [parentId]
        })
    });

    if (createRes.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    if (!createRes.ok) {
        const errText = await createRes.text();
        console.error(`[Drive] Create folder failed: ${folderName}`, errText);
        throw new Error(`Create folder failed: ${createRes.status}`);
    }
    const folderData = await createRes.json();
    return folderData.id;
}

export async function find_or_create_folder(folderName, parentId = 'root') {
    const headers = await getHeaders();
    console.log(`[Drive] Searching for folder: "${folderName}" in parent: "${parentId}"`);
    
    const query = `mimeType='${FOLDER_MIME_TYPE}' and name='${folderName}' and '${parentId}' in parents and trashed=false`;
    const searchRes = await fetch(`https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`, { headers });
    
    if (searchRes.status === 401) {
        console.warn("[Drive] 401 Unauthorized. Signing out...");
        sign_out();
        throw new Error("UNAUTHORIZED");
    }

    if (!searchRes.ok) {
        const errText = await searchRes.text();
        console.error(`[Drive] Search folder failed: ${folderName}`, errText);
        throw new Error(`Search folder failed: ${searchRes.status}`);
    }
    const searchData = await searchRes.json();
    
    if (searchData.files && searchData.files.length > 0) {
        const id = searchData.files[0].id;
        console.log(`[Drive] Found existing folder: "${folderName}" (ID: ${id})`);
        return id;
    }
    
    console.log(`[Drive] Folder "${folderName}" not found. Creating in parent: "${parentId}"`);
    const createRes = await fetch('https://www.googleapis.com/drive/v3/files', {
        method: 'POST',
        headers,
        body: JSON.stringify({
            name: folderName,
            mimeType: FOLDER_MIME_TYPE,
            parents: [parentId]
        })
    });

    if (createRes.status === 401) {
        sign_out();
        throw new Error("UNAUTHORIZED");
    }

    if (!createRes.ok) {
        const errText = await createRes.text();
        console.error(`[Drive] Create folder failed: ${folderName}`, errText);
        throw new Error(`Create folder failed: ${createRes.status}`);
    }
    const folderData = await createRes.json();
    console.log(`[Drive] Successfully created folder: "${folderName}" (ID: ${folderData.id})`);
    return folderData.id;
}

export async function ensure_directory_structure() {
    console.log("[Drive] Starting directory structure verification...");
    try {
        const appSupportId = await find_or_create_folder('ApplicationSupport', 'root');
        const leafDataId = await find_or_create_folder('LeafData', appSupportId);
        const noCategoryId = await find_or_create_folder('NO_CATEGORY', leafDataId);
        console.log("[Drive] Directory structure is ready.");
        return { appSupportId, leafDataId, noCategoryId };
    } catch (e) {
        console.error("[Drive] Directory structure setup failed:", e);
        throw e;
    }
}

/**
 * 内部用：マルチパートボディの構築
 */
function buildMultipartBody(filename, content, folderId, boundary) {
    const encoder = new TextEncoder();
    const metadata = {
        name: filename,
        mimeType: FILE_MIME_TYPE
    };
    if (folderId) metadata.parents = [folderId];

    const part1 = `--${boundary}\r\n` +
                  `Content-Type: application/json; charset=UTF-8\r\n\r\n` +
                  `${JSON.stringify(metadata)}\r\n`;
    
    const part2 = `--${boundary}\r\n` +
                  `Content-Type: ${FILE_MIME_TYPE}\r\n\r\n`;
    
    const end = `\r\n--${boundary}--`;

    return new Blob([
        encoder.encode(part1),
        encoder.encode(part2),
        encoder.encode(content),
        encoder.encode(end)
    ], { type: `multipart/related; boundary=${boundary}` });
}

export async function move_file(fileId, oldParentId, newParentId) {
    const token = get_access_token();
    if (!token) throw new Error("No access token");

    const url = `https://www.googleapis.com/drive/v3/files/${fileId}?addParents=${newParentId}&removeParents=${oldParentId}&fields=id,parents`;
    const response = await fetch(url, {
        method: 'PATCH',
        headers: {
            'Authorization': `Bearer ${token}`
        }
    });

    if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    if (!response.ok) {
        const err = await response.text();
        console.error("[Drive] Move failed:", response.status, err);
        throw new Error(`Move failed: ${response.status}`);
    }

    console.log("[Drive] File moved successfully.");
    return await response.json();
}

export async function upload_file(filename, content, folderId, fileId = null) {
    const token = get_access_token();
    if (!token) throw new Error("No access token");

    console.log(`[Drive] Uploading file: "${filename}" (fileId: ${fileId})`);

    // --- ケース1: 上書き保存 (PATCH) ---
    if (fileId) {
        // コンテンツのみを更新するシンプルアップロード
        const response = await fetch(`https://www.googleapis.com/upload/drive/v3/files/${fileId}?uploadType=media&fields=id,name,modifiedTime`, {
            method: 'PATCH',
            headers: {
                'Authorization': `Bearer ${token}`,
                'Content-Type': FILE_MIME_TYPE
            },
            body: content
        });

        if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
        
        // 成功
        if (response.ok) {
            console.log("[Drive] PATCH successful.");
            return await response.json();
        }

        // 404エラーの場合は削除されたと判断し、新規作成へフォールバック
        if (response.status === 404) {
            console.warn(`[Drive] File ID ${fileId} not found. Falling back to creation.`);
        } else {
            const err = await response.text();
            console.error("[Drive] PATCH failed:", response.status, err);
            throw new Error(`Upload failed: ${response.status}`);
        }
    }

    // --- ケース2: 新規作成 (POST) または 404からのリトライ ---
    const boundary = '-------314159265358979323846';
    const body = buildMultipartBody(filename, content, folderId, boundary);

    const response = await fetch(`https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,name,modifiedTime`, {
        method: 'POST',
        headers: {
            'Authorization': `Bearer ${token}`
        },
        body: body
    });

    if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    if (!response.ok) {
        const err = await response.text();
        console.error("[Drive] POST failed:", response.status, err);
        throw new Error(`Upload failed: ${response.status}`);
    }

    console.log("[Drive] POST successful.");
    return await response.json();
}

export async function list_files(folderId) {
    const headers = await getHeaders();
    const query = `'${folderId}' in parents and mimeType != '${FOLDER_MIME_TYPE}' and trashed=false`;
    const response = await fetch(`https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`, { headers });
    if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    return await response.json();
}

export function parse_date(dateStr) {
    return Date.parse(dateStr);
}

export async function download_file(fileId, range = null) {
    const token = get_access_token();
    if (!token) throw new Error("No access token");
    
    const headers = { 'Authorization': `Bearer ${token}` };
    if (range) headers['Range'] = `bytes=${range}`;

    const response = await fetch(`https://www.googleapis.com/drive/v3/files/${fileId}?alt=media`, { headers });
    if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    if (!response.ok && response.status !== 206) throw new Error(`Download failed: ${response.status}`);

    const buffer = await response.arrayBuffer();
    const decoder = new TextDecoder('utf-8');
    let text = decoder.decode(buffer);
    if (text.charCodeAt(0) === 0xFEFF) text = text.slice(1);
    return text;
}

export async function get_file_metadata(fileId) {
    const headers = await getHeaders();
    const response = await fetch(`https://www.googleapis.com/drive/v3/files/${fileId}?fields=id,name,modifiedTime,trashed,parents`, { headers });
    if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    return await response.json();
}
