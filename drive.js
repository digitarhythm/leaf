// drive.js
// Google Drive API wrapper with automatic token refresh

import { get_access_token, try_silent_refresh, sign_out } from './auth.js';

export const FOLDER_MIME_TYPE = 'application/vnd.google-apps.folder';
const FILE_MIME_TYPE = 'text/plain';

/**
 * 認証付きフェッチ。401エラー時に自動でリフレッシュしてリトライする。
 */
async function authenticatedFetch(url, options = {}, retry = true) {
    const token = get_access_token();
    if (!token) throw new Error("No access token available");

    const headers = {
        'Authorization': `Bearer ${token}`,
        ...options.headers
    };

    const response = await fetch(url, { ...options, headers });

    if (response.status === 401 && retry) {
        console.warn("[Drive] 401 Unauthorized. Attempting automatic refresh...");
        try {
            await try_silent_refresh();
            // 新しいトークンで再試行
            return await authenticatedFetch(url, options, false);
        } catch (e) {
            console.error("[Drive] Automatic refresh failed. Signing out.");
            sign_out();
            throw new Error("UNAUTHORIZED");
        }
    }

    if (response.status === 401 && !retry) {
        console.error("[Drive] 401 Unauthorized even after refresh. Signing out.");
        sign_out();
        throw new Error("UNAUTHORIZED");
    }

    return response;
}

export async function list_folders(parentId = 'root') {
    const query = `'${parentId}' in parents and mimeType = '${FOLDER_MIME_TYPE}' and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`;
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
    console.log(`[Drive] Searching for folder: "${folderName}" in parent: "${parentId}"`);
    
    const query = `mimeType='${FOLDER_MIME_TYPE}' and name='${folderName}' and '${parentId}' in parents and trashed=false`;
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`;
    const searchRes = await authenticatedFetch(url);
    
    if (!searchRes.ok) throw new Error(`Search folder failed: ${searchRes.status}`);
    const searchData = await searchRes.json();
    
    if (searchData.files && searchData.files.length > 0) {
        return searchData.files[0].id;
    }
    
    console.log(`[Drive] Folder "${folderName}" not found. Creating...`);
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

export async function ensure_directory_structure() {
    try {
        const appSupportId = await find_or_create_folder('ApplicationSupport', 'root');
        const leafDataId = await find_or_create_folder('LeafData', appSupportId);
        const othersId = await find_or_create_folder('OTHERS', leafDataId);
        return { appSupportId, leafDataId, othersId };
    } catch (e) {
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

    return new Blob([encoder.encode(part1), encoder.encode(part2), encoder.encode(content), encoder.encode(end)], 
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

export async function upload_file(filename, content, folderId, fileId = null) {
    if (fileId) {
        const url = `https://www.googleapis.com/upload/drive/v3/files/${fileId}?uploadType=media&fields=id,name,modifiedTime`;
        const response = await authenticatedFetch(url, {
            method: 'PATCH',
            headers: { 'Content-Type': FILE_MIME_TYPE },
            body: content
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
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name, modifiedTime)`;
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
    const url = `https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`;
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
        
        // 416 Range Not Satisfiable: ファイルが空（0バイト）の場合にRange指定すると発生する
        if (response.status === 416) return "";
        
        if (!response.ok && response.status !== 206) {
            console.warn(`[Drive] Download failed with status ${response.status} for file ${fileId}`);
            return "";
        }

        const buffer = await response.arrayBuffer();
        const decoder = new TextDecoder('utf-8');
        let text = decoder.decode(buffer);
        if (text.charCodeAt(0) === 0xFEFF) text = text.slice(1);
        return text;
    } catch (e) {
        if (e.name === 'AbortError') return "";
        console.error(`[Drive] download_file error for ${fileId}:`, e);
        return "";
    }
}

export async function get_file_metadata(fileId) {
    const url = `https://www.googleapis.com/drive/v3/files/${fileId}?fields=id,name,modifiedTime,trashed,parents`;
    const response = await authenticatedFetch(url);
    if (!response.ok) throw new Error(`Get metadata failed: ${response.status}`);
    return await response.json();
}
