use rusqlite::{params, Connection};
use serde::Serialize;
use std::{fs, path::{Path, PathBuf}};
use sysinfo::Disks;
use tauri::Manager;
use walkdir::WalkDir;

#[derive(Serialize)]
struct Game { id: i64, title: String, platform: String, path: String, file_size: i64, online: bool }
#[derive(Serialize)]
struct Drive { name: String, mount_point: String, total_bytes: u64, available_bytes: u64, removable: bool }

fn catalog_path(handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = handle.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    Ok(directory.join("retrodrive.sqlite"))
}
fn database(handle: &tauri::AppHandle) -> Result<Connection, String> {
    let connection = Connection::open(catalog_path(handle)?).map_err(|e| e.to_string())?;
    connection.execute_batch("CREATE TABLE IF NOT EXISTS game_files (
        id INTEGER PRIMARY KEY,
        path TEXT NOT NULL UNIQUE,
        title TEXT NOT NULL,
        platform TEXT NOT NULL,
        file_size INTEGER NOT NULL DEFAULT 0,
        last_seen TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
    ); CREATE INDEX IF NOT EXISTS idx_game_files_platform ON game_files(platform);").map_err(|e| e.to_string())?;
    Ok(connection)
}
#[tauri::command]
fn list_games(handle: tauri::AppHandle) -> Result<Vec<Game>, String> {
    let connection = database(&handle)?;
    let mut statement = connection.prepare("SELECT id,title,platform,path,file_size FROM game_files ORDER BY title COLLATE NOCASE")
        .map_err(|e| e.to_string())?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, i64>(4)?)))
        .map_err(|e| e.to_string())?;
    rows.map(|row| {
        let (id,title,platform,path,file_size) = row.map_err(|e| e.to_string())?;
        let online = Path::new(&path).is_file();
        Ok(Game { id,title,platform,path,file_size,online })
    }).collect()
}
fn platform_for_extension(extension: &str, parent: &str) -> Option<&'static str> {
    let folder = parent.to_lowercase();
    let from_folder = match folder.as_str() {
        "nes" | "fc" => Some("NES"), "snes" | "sfc" => Some("SNES"),
        "megadrive" | "genesis" | "md" => Some("Genesis"),
        "arcade" | "mame" | "fbneo" => Some("Arcade"),
        "psx" | "ps1" | "psone" => Some("PlayStation"),
        "ps2" => Some("PlayStation 2"), "gba" => Some("GBA"),
        "gb" => Some("Game Boy"), "gbc" => Some("Game Boy Color"),
        "n64" => Some("N64"), "mastersystem" => Some("Master System"),
        "gamegear" => Some("Game Gear"), _ => None
    };
    if let Some(p) = from_folder { return Some(p); }
    match extension {
        "nes" => Some("NES"), "sfc" | "smc" => Some("SNES"),
        "gen" | "md" | "smd" => Some("Genesis"),
        "gba" => Some("GBA"), "gb" => Some("Game Boy"),
        "gbc" => Some("Game Boy Color"), "n64" | "z64" | "v64" => Some("N64"),
        "cue" | "chd" | "iso" | "zip" | "7z" => Some("Unclassified"),
        _ => None
    }
}
#[tauri::command]
fn scan_library(handle: tauri::AppHandle, folder: String) -> Result<usize, String> {
    let root = PathBuf::from(folder);
    if !root.is_dir() { return Err("The selected location is not a directory".into()); }
    let mut connection = database(&handle)?;
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    let mut count = 0;
    // Do not follow symbolic links. Never modify scanned game folders.
    for entry in WalkDir::new(&root).follow_links(false).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        let parent = path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("");
        let Some(platform) = platform_for_extension(&extension, parent) else { continue; };
        let title = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").replace('_', " ");
        let size = entry.metadata().map(|m| m.len() as i64).unwrap_or(0);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().into_owned();
        transaction.execute("INSERT INTO game_files (path,title,platform,file_size) VALUES (?1,?2,?3,?4)
            ON CONFLICT(path) DO UPDATE SET title=excluded.title, platform=excluded.platform, file_size=excluded.file_size, last_seen=CURRENT_TIMESTAMP",
            params![canonical,title,platform,size]).map_err(|e| e.to_string())?;
        count += 1;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(count)
}
#[tauri::command]
fn list_drives() -> Vec<Drive> {
    let disks = Disks::new_with_refreshed_list();
    disks.list().iter().map(|disk| Drive {
        name: disk.name().to_string_lossy().into_owned(),
        mount_point: disk.mount_point().to_string_lossy().into_owned(),
        total_bytes: disk.total_space(), available_bytes: disk.available_space(),
        removable: disk.is_removable()
    }).collect()
}
pub fn run() {
    tauri::Builder::default()
      .plugin(tauri_plugin_dialog::init())
      .invoke_handler(tauri::generate_handler![list_games,scan_library,list_drives])
      .run(tauri::generate_context!())
      .expect("error starting RetroDrive Studio");
}
