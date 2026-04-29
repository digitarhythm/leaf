use tauri_plugin_sql::{Builder as SqlBuilder, Migration, MigrationKind};
use std::time::Duration; // rebuild
use std::sync::{Arc, Mutex};
use serde_json::Value;
use tauri_plugin_dialog::DialogExt;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl, class};

// 複数PTY状態管理
struct PtyInstance {
    writer: Box<dyn std::io::Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
}

static PTY_INSTANCES: once_cell::sync::Lazy<Arc<Mutex<std::collections::HashMap<String, PtyInstance>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(std::collections::HashMap::new())));

// Desktop App 用の OAuth リダイレクトポート
#[allow(dead_code)]
const OAUTH_PORT: u16 = 3456;

// 本番サーバーのAPI URL
const BACKEND_URL: &str = "https://leaf.digitarhythm.net";


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
fn set_window_opacity(app: tauri::AppHandle, opacity: f64) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        use cocoa::appkit::{NSWindow, CGFloat};
        if let Some(window) = app.get_webview_window("main") {
            let ns_window = window.ns_window().map_err(|e| format!("{}", e))?;
            let alpha = opacity.clamp(0.5, 1.0);
            unsafe {
                let ns_win: cocoa::base::id = ns_window as cocoa::base::id;
                ns_win.setAlphaValue_(alpha as CGFloat);
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("main") {
            let hwnd = window.hwnd().map_err(|e| format!("{}", e))?;
            // opacityは0.0〜1.0 (50%〜100%で渡される)、255=不透明 0=透明
            let alpha = (opacity.clamp(0.5, 1.0) * 255.0) as u8;
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::*;
                let style = GetWindowLongW(hwnd.0 as _, GWL_EXSTYLE);
                SetWindowLongW(hwnd.0 as _, GWL_EXSTYLE, style | WS_EX_LAYERED as i32);
                SetLayeredWindowAttributes(hwnd.0 as _, 0, alpha, LWA_ALPHA);
            }
        }
    }
    #[cfg(target_os = "linux")]
    { let _ = (app, opacity); }
    Ok(())
}

#[tauri::command]
fn set_window_blur(app: tauri::AppHandle, blur: i32) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("main") {
            if blur > 0 {
                let opacity_byte = (255.0 * (1.0 - blur as f64 / 100.0)) as u8;
                // 既存のエフェクトをクリアしてから適用（重複適用を避ける）
                let _ = window_vibrancy::clear_mica(&window);
                let _ = window_vibrancy::clear_acrylic(&window);
                let _ = window_vibrancy::clear_blur(&window);

                // Win11: Mica → 失敗時 Acrylic → さらに失敗時 Blur (Win10)
                let mica_res = window_vibrancy::apply_mica(&window, Some(true));
                if let Err(ref e) = mica_res {
                    println!("[Tauri] apply_mica failed: {:?}", e);
                    let acrylic_res = window_vibrancy::apply_acrylic(&window, Some((0, 0, 0, opacity_byte)));
                    if let Err(ref e2) = acrylic_res {
                        println!("[Tauri] apply_acrylic failed: {:?}", e2);
                        if let Err(e3) = window_vibrancy::apply_blur(&window, Some((0, 0, 0, opacity_byte))) {
                            println!("[Tauri] apply_blur failed: {:?}", e3);
                            return Err(format!("blur effect failed: mica={:?} acrylic={:?} blur={:?}", mica_res, acrylic_res, e3));
                        } else {
                            println!("[Tauri] apply_blur OK");
                        }
                    } else {
                        println!("[Tauri] apply_acrylic OK");
                    }
                } else {
                    println!("[Tauri] apply_mica OK");
                }
            } else {
                let _ = window_vibrancy::clear_mica(&window);
                let _ = window_vibrancy::clear_acrylic(&window);
                let _ = window_vibrancy::clear_blur(&window);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    { let _ = (app, blur); }
    Ok(())
}

#[tauri::command]
fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

#[tauri::command]
fn is_windows() -> bool {
    cfg!(target_os = "windows")
}

#[tauri::command]
fn pty_spawn(app: tauri::AppHandle, id: String, cols: u16, rows: u16) -> Result<(), String> {
    use std::io::Read;
    use tauri::Emitter;

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| format!("Failed to open PTY: {}", e))?;

    let shell = get_default_shell();
    let mut cmd = CommandBuilder::new(&shell);
    #[cfg(target_os = "windows")]
    {
        // WSLの場合は-eオプションでbashを直接起動
        if shell.contains("wsl") {
            cmd.arg("-e");
            cmd.arg("bash");
            cmd.arg("--login");
        }
    }
    cmd.env("TERM", "xterm-256color");
    if let Some(home) = dirs::home_dir() { cmd.cwd(home); }

    let mut child = pair.slave.spawn_command(cmd).map_err(|e| format!("Failed to spawn: {}", e))?;
    let mut reader = pair.master.try_clone_reader().map_err(|e| format!("{}", e))?;
    let writer = pair.master.take_writer().map_err(|e| format!("{}", e))?;
    let master = pair.master;
    drop(pair.slave);
    // childスレッド: プロセス終了を監視（Windowsのwsl等でreaderが終了を検知できない場合の保険）
    let pty_id_child = id.clone();
    let app_handle_child = app.clone();
    std::thread::spawn(move || {
        let _ = child.wait();
        let _ = app_handle_child.emit("pty-exit", serde_json::json!({ "id": pty_id_child }));
    });

    let pty_id = id.clone();
    let app_handle = app.clone();
    // readerスレッド: PTY出力をJSへ転送
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    use base64::Engine;
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                    let _ = app_handle.emit("pty-output", serde_json::json!({ "id": pty_id, "data": encoded }));
                }
                Err(_) => break,
            }
        }
        let _ = app_handle.emit("pty-exit", serde_json::json!({ "id": pty_id }));
    });

    let mut instances = PTY_INSTANCES.lock().map_err(|e| format!("{}", e))?;
    instances.insert(id, PtyInstance { writer, master });
    Ok(())
}

#[tauri::command]
fn pty_write(id: String, data: String) -> Result<(), String> {
    use std::io::Write;
    let mut instances = PTY_INSTANCES.lock().map_err(|e| format!("{}", e))?;
    if let Some(inst) = instances.get_mut(&id) {
        if let Err(_) = inst.writer.write_all(data.as_bytes()) {
            // シェルが終了済み — インスタンスを除去して静かに返す
            let _ = instances.remove(&id);
            return Ok(());
        }
        let _ = inst.writer.flush();
    }
    Ok(())
}

#[tauri::command]
fn pty_resize(id: String, cols: u16, rows: u16) -> Result<(), String> {
    let instances = PTY_INSTANCES.lock().map_err(|e| format!("{}", e))?;
    if let Some(inst) = instances.get(&id) {
        inst.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("Failed to resize PTY: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn pty_kill(id: String) -> Result<(), String> {
    let mut instances = PTY_INSTANCES.lock().map_err(|e| format!("{}", e))?;
    // masterを閉じることでPTYを終了させる（childスレッドがwaitで検知）
    let _ = instances.remove(&id);
    Ok(())
}

fn get_default_shell() -> String {
    #[cfg(target_os = "windows")]
    {
        // WSL2が利用可能ならwsl.exeを使用（pty_spawnで-e bashオプション付き）
        if let Ok(output) = std::process::Command::new("wsl").arg("--status").output() {
            if output.status.success() {
                return "wsl".to_string();
            }
        }
        // WSL未インストールの場合はPowerShellにフォールバック
        return "powershell.exe".to_string();
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
    }
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

            // UTF-8 BOM (EF BB BF) があれば content から剥がす。
            // bytes 側は保持（呼び出し側で BOM 判定して needs_bom を決めるため）。
            let content_slice: &[u8] = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                &bytes[3..]
            } else {
                &bytes[..]
            };

            let content = match std::str::from_utf8(content_slice) {
                Ok(s) => s.to_string(),
                Err(_) => {
                    println!("[Tauri] UTF-8 failed, falling back to lossy conversion...");
                    String::from_utf8_lossy(content_slice).to_string()
                }
            };

            Ok(serde_json::json!({
                "name": name,
                "content": content,
                "bytes": bytes,
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

#[tauri::command]
fn open_url_in_browser(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

#[tauri::command]
fn open_local_page(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let label = format!("local-page-{}", js_sys_date_now_approx());
    tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App(path.into()),
    )
    .title("Leaf")
    .inner_size(960.0, 720.0)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn js_sys_date_now_approx() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
            save_local_file_native,
            open_url_in_browser,
            open_local_page,
            set_window_opacity,
            set_window_blur,
            is_macos,
            is_windows,
            pty_spawn,
            pty_write,
            pty_resize,
            pty_kill
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
