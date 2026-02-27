use tauri_plugin_sql::{Builder as SqlBuilder, Migration, MigrationKind};
use std::time::Duration;
use serde_json::Value;
use tauri_plugin_dialog::DialogExt;

// Desktop App 用の OAuth リダイレクトポート
const OAUTH_PORT: u16 = 3456;

// フロントエンドのバックエンドプロキシ (server/index.js) 用のURL
const BACKEND_URL: &str = "http://127.0.0.1:3000";


#[tauri::command]
fn init_db() -> Result<(), String> {
    println!("init_db called");
    Ok(())
}

#[tauri::command]
fn log_from_js(msg: String) {
    eprintln!("[JS_ERROR] {}", msg);
}

#[tauri::command]
fn save_sheet_to_db(_sheet: serde_json::Value) -> Result<(), String> {
    println!("save_sheet_to_db called");
    Ok(())
}

#[tauri::command]
fn load_sheets_from_db() -> Result<Vec<serde_json::Value>, String> {
    println!("load_sheets_from_db called");
    Ok(vec![])
}

#[tauri::command]
fn delete_sheet_from_db(_id: String) -> Result<(), String> {
    println!("delete_sheet_from_db called");
    Ok(())
}

#[tauri::command]
fn save_categories_to_db(_categories: Vec<serde_json::Value>) -> Result<(), String> {
    println!("save_categories_to_db called");
    Ok(())
}

#[tauri::command]
fn load_categories_from_db() -> Result<Vec<serde_json::Value>, String> {
    println!("load_categories_from_db called");
    Ok(vec![])
}

#[tauri::command]
#[allow(non_snake_case)]
async fn authenticate_google(app: tauri::AppHandle, clientId: Option<String>) -> Result<String, String> {
    authenticate_google_force(app, clientId).await
}

#[tauri::command]
#[allow(non_snake_case)]
async fn authenticate_google_force(_app: tauri::AppHandle, clientId: Option<String>) -> Result<String, String> {
    println!("authenticate_google_force called");

    // Use passed clientId or fallback to environment variable
    let active_client_id = clientId
        .or_else(|| std::env::var("LEAF_CLIENTID").ok())
        .ok_or_else(|| "LEAF_CLIENTID not set and no clientId provided".to_string())?;

    // Start OAuth server with v2 closure API on a fixed port so Google redirect works
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let _port = tauri_plugin_oauth::start_with_config(
        tauri_plugin_oauth::OauthConfig {
            ports: Some(vec![3456]),
            response: Some(
                "<html><body><h2>Authentication successful! You can close this window.</h2><script>window.close();</script></body></html>".into()
            ),
        },
        move |url| {
            let _ = tx.send(url);
        }
    ).map_err(|e| format!("Failed to start OAuth server: {}", e))?;

    // GCPに登録されている正確なリダイレクトURL (localhost:3456)
    let redirect_uri = "http://localhost:3456/auth/";

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20https://www.googleapis.com/auth/drive.file&access_type=offline&prompt=consent",
        active_client_id, url::form_urlencoded::byte_serialize(redirect_uri.as_bytes()).collect::<String>()
    );

    // Open the browser
    if let Err(e) = open::that(&auth_url) {
        return Err(format!("Failed to open browser: {}", e));
    }

    // Wait for the redirect
    let timeout_duration = Duration::from_secs(300); // 5 minutes timeout
    let code_result: Result<String, String> = match tokio::time::timeout(timeout_duration, async {
        if let Some(url_str) = rx.recv().await {
            if let Ok(url) = url::Url::parse(&url_str) {
                for (key, value) in url.query_pairs() {
                    if key == "code" {
                        return Ok(value.into_owned());
                    } else if key == "error" {
                        return Err(format!("OAuth Error: {}", value));
                    }
                }
            }
        }
        Err("URL parse error or closed".to_string())
    }).await {
        Ok(Ok(code)) => Ok(code),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Authentication timed out".to_string()),
    };

    let code = code_result?;

    println!("Received auth code, exchanging for token...");

    // Exchange code for token via local Node backend
    let client = reqwest::Client::new();
    let res = client.post(format!("{}/api/auth/token", BACKEND_URL))
        .json(&serde_json::json!({
            "code": code,
            "redirect_uri": redirect_uri // Provide the redirect URI so backend knows it's native
        }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if res.status().is_success() {
        let json: Value = res.json().await.map_err(|e| format!("JSON parse err: {}", e))?;
        println!("Token exchange successful");
        // Return the full JSON response so frontend can extract refresh_token too
        return Ok(serde_json::to_string(&json).map_err(|e| format!("Serialize err: {}", e))?);
    } else {
        let error_text = res.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {}", error_text));
    }
}

#[tauri::command]
#[allow(non_snake_case)]
async fn refresh_google_token(refreshToken: String) -> Result<String, String> {
    println!("refresh_google_token called");
    
    let client = reqwest::Client::new();
    let res = client.post(format!("{}/api/auth/refresh", BACKEND_URL))
        .json(&serde_json::json!({
            "refresh_token": refreshToken
        }))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if res.status().is_success() {
        let json: Value = res.json().await.map_err(|e| format!("JSON parse err: {}", e))?;
        if let Some(token) = json["access_token"].as_str() {
            return Ok(token.to_string());
        }
    }
    
    Err("Failed to refresh token".to_string())
}

#[tauri::command]
async fn open_local_file_native(app: tauri::AppHandle) -> Result<Value, String> {
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel();

    app.dialog().file()
        .add_filter("Text Files", &["txt", "md", "js", "ts", "rs", "toml", "json", "yaml", "yml", "sql", "html", "css", "py", "c", "cpp", "h", "m", "cs", "php", "coffee", "pl", "rb", "java", "sh", "xml"])
        .pick_file(move |file_path| {
            let _ = tx.send(file_path);
        });

    let file_path = rx.recv().map_err(|e| format!("Dialog error: {}", e))?;

    match file_path {
        Some(file_path) => {
            let path = file_path.as_path().ok_or("Invalid file path")?.to_path_buf();
            let bytes = std::fs::read(&path)
                .map_err(|e| format!("Failed to read file: {}", e))?;

            let name = path.file_name()
                .map(|n: &std::ffi::OsStr| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            // Try UTF-8 first, then Shift_JIS
            let content = match String::from_utf8(bytes.clone()) {
                Ok(s) => s,
                Err(_) => {
                    println!("[Tauri] UTF-8 failed, trying Shift_JIS...");
                    // Simple fallback: lossy UTF-8
                    String::from_utf8_lossy(&bytes).to_string()
                }
            };

            let bytes_array: Vec<u8> = bytes;
            Ok(serde_json::json!({
                "name": name,
                "content": content,
                "bytes": bytes_array,
                "path": path.to_string_lossy().to_string()
            }))
        }
        None => Err("cancelled".to_string())
    }
}

#[tauri::command]
#[allow(non_snake_case)]
async fn save_local_file_native(app: tauri::AppHandle, content: String, needsBom: bool, currentPath: Option<String>) -> Result<Value, String> {
    let path = if let Some(existing) = currentPath {
        std::path::PathBuf::from(existing)
    } else {
        use std::sync::mpsc;
        let (tx, rx) = mpsc::channel();

        app.dialog().file()
            .add_filter("Text Files", &["txt", "md", "js", "ts", "rs", "toml", "json", "yaml", "yml", "sql", "html", "css", "py"])
            .set_file_name("Untitled.txt")
            .save_file(move |file_path| {
                let _ = tx.send(file_path);
            });

        let result = rx.recv().map_err(|e| format!("Dialog error: {}", e))?;
        match result {
            Some(file_path) => file_path.as_path().ok_or("Invalid file path")?.to_path_buf(),
            None => return Err("cancelled".to_string())
        }
    };

    let mut data = Vec::new();
    if needsBom {
        data.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    data.extend_from_slice(content.as_bytes());

    std::fs::write(&path, &data)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    let name = path.file_name()
        .map(|n: &std::ffi::OsStr| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Ok(serde_json::json!({
        "name": name,
        "path": path.to_string_lossy().to_string()
    }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // .env ファイルから環境変数を読み込む
    let env_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env");
    if env_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&env_path) {
            for line in contents.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') { continue; }
                if let Some((key, value)) = line.split_once('=') {
                    std::env::set_var(key.trim(), value.trim());
                }
            }
            println!("[Tauri] Loaded .env from {:?}", env_path);
        }
    }

    let migrations = vec![
        Migration {
            version: 1,
            description: "create_initial_tables",
            sql: "
                CREATE TABLE IF NOT EXISTS sheets (
                    id TEXT PRIMARY KEY,
                    title TEXT,
                    content TEXT,
                    updated_at INTEGER,
                    folder_id TEXT,
                    is_trashed INTEGER
                );
                CREATE TABLE IF NOT EXISTS categories (
                    id TEXT PRIMARY KEY,
                    name TEXT,
                    color TEXT,
                    sort_order INTEGER
                );
            ",
            kind: MigrationKind::Up,
        }
    ];

    tauri::Builder::default()
        .plugin(SqlBuilder::default().add_migrations("sqlite:leaf.db", migrations).build())
        .plugin(tauri_plugin_oauth::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            init_db,
            save_sheet_to_db,
            load_sheets_from_db,
            delete_sheet_from_db,
            save_categories_to_db,
            load_categories_from_db,
            authenticate_google,
            authenticate_google_force,
            refresh_google_token,
            log_from_js,
            open_local_file_native,
            save_local_file_native
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
