#[tauri::command]
fn init_db() -> Result<(), String> {
    println!("init_db called");
    Ok(())
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
fn authenticate_google() -> Result<String, String> {
    println!("authenticate_google called");
    Err("Not implemented yet".to_string())
}

#[tauri::command]
fn authenticate_google_force() -> Result<String, String> {
    println!("authenticate_google_force called");
    Err("Not implemented yet".to_string())
}

#[tauri::command]
fn refresh_google_token(_refresh_token: String) -> Result<String, String> {
    println!("refresh_google_token called");
    Err("Not implemented yet".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
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
      refresh_google_token
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
