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

export async function get_root_info() {
    const headers = await getHeaders();
    const response = await fetch(`https://www.googleapis.com/drive/v3/files/root?fields=id,name`, { headers });
    return await response.json();
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

export async function upload_file(filename, content, folderId, fileId = null) {
    const token = get_access_token();
    if (!token) throw new Error("No access token");

    const metadata = {
        name: filename,
        mimeType: FILE_MIME_TYPE,
    };
    if (folderId && !fileId) {
        metadata.parents = [folderId];
    }

    const boundary = '-------314159265358979323846';
    const delimiter = "\r\n--" + boundary + "\r\n";
    const close_delim = "\r\n--" + boundary + "--";
    
    const encoder = new TextEncoder();
    const bom = new Uint8Array([0xEF, 0xBB, 0xBF]); // UTF-8 BOM
    const contentBytes = encoder.encode(content);
    
    let method = 'POST';
    let path = '/upload/drive/v3/files?uploadType=multipart';
    if (fileId) {
        method = 'PATCH';
        path = `/upload/drive/v3/files/${fileId}?uploadType=multipart`;
    }

    const metadataPart = `Content-Type: application/json\r\n\r\n${JSON.stringify(metadata)}`;
    const contentPartHeader = `Content-Type: ${FILE_MIME_TYPE}`;
    
    const bodyParts = [
        delimiter,
        metadataPart,
        delimiter,
        contentPartHeader,
        '\r\n\r\n',
        bom,
        contentBytes,
        close_delim
    ];
    
    const body = new Blob(bodyParts, { type: `multipart/related; boundary=${boundary}` });

    const response = await fetch(`https://www.googleapis.com` + path, {
        method: method,
        headers: {
            'Authorization': `Bearer ${token}`,
            'Content-Type': `multipart/related; boundary=${boundary}`
        },
        body: body
    });
    
    if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    return await response.json();
}

export async function list_files(folderId) {
    const headers = await getHeaders();
    const query = `'${folderId}' in parents and mimeType != '${FOLDER_MIME_TYPE}' and trashed=false`;
    const response = await fetch(`https://www.googleapis.com/drive/v3/files?q=${encodeURIComponent(query)}&fields=files(id, name)`, { headers });
    if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    return await response.json();
}

export async function download_file(fileId) {
    const token = get_access_token();
    if (!token) throw new Error("No access token");
    
    const response = await fetch(`https://www.googleapis.com/drive/v3/files/${fileId}?alt=media`, {
        headers: {
            'Authorization': `Bearer ${token}`
        }
    });
    
    if (response.status === 401) { sign_out(); throw new Error("UNAUTHORIZED"); }
    const buffer = await response.arrayBuffer();
    const decoder = new TextDecoder('utf-8');
    
    let text = decoder.decode(buffer);
    if (text.charCodeAt(0) === 0xFEFF) {
        text = text.slice(1);
    }
    
    return text;
}
