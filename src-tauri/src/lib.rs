mod api;
mod db;
mod providers;

use api::anilist::{Anime, AniListClient};
use db::{Database, DownloadEntry, MokurokuEntry};
use providers::allanime::{AllAnimeClient, AllAnimeShow, EpisodeSource};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::io::AsyncWriteExt;

struct AppState {
    anilist: AniListClient,
    allanime: AllAnimeClient,
    db: Database,
    download_semaphore: Arc<tokio::sync::Semaphore>,
}

impl AppState {
    fn allow_adult(&self) -> bool {
        self.db
            .get_setting("allow_adult")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false)
    }

    fn download_dir(&self) -> PathBuf {
        let dir = self
            .db
            .get_setting("download_dir")
            .ok()
            .flatten()
            .unwrap_or_else(|| "~/Shizuku".to_string());
        let expanded = if dir.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                home.join(&dir[2..])
            } else {
                PathBuf::from(&dir)
            }
        } else {
            PathBuf::from(&dir)
        };
        expanded
    }
}

// --- AniList commands ---

#[tauri::command]
async fn search_anime(
    state: tauri::State<'_, Arc<AppState>>,
    query: String,
) -> Result<Vec<Anime>, String> {
    let allow_adult = state.allow_adult();
    let cache_key = format!("search:{}:adult={}", query, allow_adult);
    if let Ok(Some(cached)) = state.db.get_cache(&cache_key) {
        if let Ok(parsed) = serde_json::from_str::<Vec<Anime>>(&cached) {
            return Ok(parsed);
        }
    }
    let result = state.anilist.search(&query, 1, 20, allow_adult).await.map_err(|e| e.to_string())?;
    if let Ok(json) = serde_json::to_string(&result) {
        let _ = state.db.set_cache(&cache_key, &json, 600);
    }
    Ok(result)
}

#[tauri::command]
async fn get_trending(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<Anime>, String> {
    let allow_adult = state.allow_adult();
    let cache_key = format!("trending:adult={}", allow_adult);
    if let Ok(Some(cached)) = state.db.get_cache(&cache_key) {
        if let Ok(parsed) = serde_json::from_str::<Vec<Anime>>(&cached) {
            return Ok(parsed);
        }
    }
    let result = state.anilist.trending(1, 20, allow_adult).await.map_err(|e| e.to_string())?;
    if let Ok(json) = serde_json::to_string(&result) {
        let _ = state.db.set_cache(&cache_key, &json, 1800);
    }
    Ok(result)
}

#[tauri::command]
async fn get_recently_updated(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<Anime>, String> {
    let allow_adult = state.allow_adult();
    let cache_key = format!("recently_updated:adult={}", allow_adult);
    if let Ok(Some(cached)) = state.db.get_cache(&cache_key) {
        if let Ok(parsed) = serde_json::from_str::<Vec<Anime>>(&cached) {
            return Ok(parsed);
        }
    }
    let result = state.anilist.recently_updated(1, 20, allow_adult).await.map_err(|e| e.to_string())?;
    if let Ok(json) = serde_json::to_string(&result) {
        let _ = state.db.set_cache(&cache_key, &json, 1800);
    }
    Ok(result)
}

#[tauri::command]
async fn get_anime_detail(
    state: tauri::State<'_, Arc<AppState>>,
    id: i64,
) -> Result<Anime, String> {
    state
        .anilist
        .get_anime(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn browse_genre(
    state: tauri::State<'_, Arc<AppState>>,
    genre: String,
    page: i32,
) -> Result<Vec<Anime>, String> {
    let allow_adult = state.allow_adult();
    let cache_key = format!("genre:{}:{}:adult={}", genre, page, allow_adult);
    if let Ok(Some(cached)) = state.db.get_cache(&cache_key) {
        if let Ok(parsed) = serde_json::from_str::<Vec<Anime>>(&cached) {
            return Ok(parsed);
        }
    }
    let result = state.anilist.browse_by_genre(&genre, page, 30, allow_adult).await.map_err(|e| e.to_string())?;
    if let Ok(json) = serde_json::to_string(&result) {
        let _ = state.db.set_cache(&cache_key, &json, 1800);
    }
    Ok(result)
}

// --- AllAnime provider commands ---

#[tauri::command]
async fn provider_search(
    state: tauri::State<'_, Arc<AppState>>,
    query: String,
    mode: String,
) -> Result<Vec<AllAnimeShow>, String> {
    state
        .allanime
        .search(&query, &mode)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_episodes(
    state: tauri::State<'_, Arc<AppState>>,
    show_id: String,
    mode: String,
) -> Result<Vec<String>, String> {
    state
        .allanime
        .get_episodes(&show_id, &mode)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_episode_sources(
    state: tauri::State<'_, Arc<AppState>>,
    show_id: String,
    episode: String,
    mode: String,
) -> Result<Vec<EpisodeSource>, String> {
    state
        .allanime
        .get_episode_sources(&show_id, &episode, &mode)
        .await
        .map_err(|e| e.to_string())
}

// --- History commands ---

#[tauri::command]
async fn add_to_history(
    state: tauri::State<'_, Arc<AppState>>,
    anime_id: i64,
    title: String,
    episode: String,
) -> Result<(), String> {
    state
        .db
        .add_to_history(anime_id, &title, &episode)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_history(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<(i64, String, String, String)>, String> {
    state.db.get_history(50).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_continue_watching(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<(i64, String, String, String)>, String> {
    state.db.get_continue_watching(10).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_watched_episodes(
    state: tauri::State<'_, Arc<AppState>>,
    anime_id: i64,
) -> Result<Vec<String>, String> {
    state.db.get_watched_episodes(anime_id).map_err(|e| e.to_string())
}

// --- Download commands ---

#[tauri::command]
async fn get_downloads(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<DownloadEntry>, String> {
    state.db.get_downloads().map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_local_file(
    state: tauri::State<'_, Arc<AppState>>,
    anime_id: i64,
    episode: String,
) -> Result<Option<String>, String> {
    let path = state
        .db
        .get_download_path(anime_id, &episode)
        .map_err(|e| e.to_string())?;
    if let Some(ref p) = path {
        if !std::path::Path::new(p).exists() {
            return Ok(None);
        }
    }
    Ok(path)
}

// Fix 4: barrel parameter for organized folders
// Fix 6: returns format string ("mp4" or "m3u8") for frontend warnings
#[tauri::command]
async fn start_download(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    show_id: String,
    anime_id: i64,
    anime_title: String,
    episode: String,
    mode: String,
    barrel: String,
    series_title: String,
) -> Result<String, String> {
    // Check DB first — skip if already complete or in-progress (avoids unnecessary API calls)
    let download_id = match state
        .db
        .queue_download(
            anime_id,
            &format!("{} - Episode {}", anime_title, episode),
            &episode,
            &barrel,
            &series_title,
        )
        .map_err(|e| e.to_string())?
    {
        Some(id) => id,
        None => return Ok("skipped".to_string()),
    };

    // Now fetch sources (only for episodes that actually need downloading)
    let sources = state
        .allanime
        .get_episode_sources(&show_id, &episode, &mode)
        .await
        .map_err(|e| {
            // Mark as failed if we can't get sources
            let _ = state.db.update_download_failed(download_id);
            e.to_string()
        })?;

    if sources.is_empty() {
        let _ = state.db.update_download_failed(download_id);
        return Err("No sources available".to_string());
    }

    let source = sources
        .iter()
        .find(|s| s.url.contains(".mp4"))
        .or(sources.first())
        .cloned()
        .ok_or_else(|| {
            let _ = state.db.update_download_failed(download_id);
            "No source found".to_string()
        })?;

    // Create download directory with barrel subfolder
    let download_dir = state.download_dir();
    let folder_name = if series_title.is_empty() { &anime_title } else { &series_title };
    let safe_title = folder_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .collect::<String>();
    let anime_dir = download_dir.join(&safe_title);
    let target_dir = if barrel.is_empty() {
        anime_dir
    } else {
        anime_dir.join(&barrel)
    };
    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    let file_name = format!("Episode_{}.mp4", episode);
    let file_path = target_dir.join(&file_name);
    let file_path_str = file_path.to_string_lossy().to_string();

    let is_m3u8 = source.url.contains("m3u8");

    if is_m3u8 {
        let url = source.url.clone();
        let fp = file_path_str.clone();
        let ah = app_handle.clone();
        let db_ref = state.inner().clone();
        let sem = state.download_semaphore.clone();

        tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let status = if command_exists("yt-dlp") {
                tokio::process::Command::new("yt-dlp")
                    .arg(&url)
                    .arg("--no-skip-unavailable-fragments")
                    .arg("-N").arg("16")
                    .arg("-o").arg(&fp)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await
            } else {
                tokio::process::Command::new("ffmpeg")
                    .arg("-i").arg(&url)
                    .arg("-c").arg("copy")
                    .arg("-loglevel").arg("error")
                    .arg(&fp)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .await
            };

            match status {
                Ok(s) if s.success() => {
                    let _ = db_ref.db.update_download_complete(download_id, &fp);
                    let _ = ah.emit("download-complete", serde_json::json!({ "id": download_id, "file_path": fp }));
                }
                _ => {
                    let _ = db_ref.db.update_download_failed(download_id);
                    let _ = ah.emit("download-failed", serde_json::json!({ "id": download_id }));
                }
            }
        });

        return Ok("m3u8".to_string());
    }

    // Direct download via reqwest streaming
    let url = source.url.clone();
    let fp = file_path_str.clone();
    let ah = app_handle.clone();
    let db_ref = state.inner().clone();
    let sem = state.download_semaphore.clone();

    tokio::spawn(async move {
        let _permit = sem.acquire().await.unwrap();
        match download_file(&url, &fp, download_id, &ah, &db_ref).await {
            Ok(()) => {
                let _ = db_ref.db.update_download_complete(download_id, &fp);
                let _ = ah.emit("download-complete", serde_json::json!({ "id": download_id, "file_path": fp }));
            }
            Err(_) => {
                let _ = db_ref.db.update_download_failed(download_id);
                let _ = ah.emit("download-failed", serde_json::json!({ "id": download_id }));
            }
        }
    });

    Ok("mp4".to_string())
}

async fn download_file(
    url: &str,
    file_path: &str,
    download_id: i64,
    app_handle: &tauri::AppHandle,
    state: &Arc<AppState>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use futures_util::StreamExt;

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:131.0) Gecko/20100101 Firefox/131.0")
        .build()?;
    let resp = client
        .get(url)
        .header("Referer", "https://allmanga.to")
        .send()
        .await?;

    // Check HTTP status before streaming
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()).into());
    }

    let total_size = resp.content_length().unwrap_or(0);

    let mut file = tokio::fs::File::create(file_path).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        if downloaded - last_emit > 102400 {
            let progress = if total_size > 0 {
                downloaded as f64 / total_size as f64
            } else {
                -1.0
            };
            let _ = state.db.update_download_progress(
                download_id,
                if progress >= 0.0 { progress } else { 0.0 },
            );
            let _ = app_handle.emit(
                "download-progress",
                serde_json::json!({
                    "id": download_id,
                    "progress": progress,
                    "downloaded": downloaded
                }),
            );
            last_emit = downloaded;
        }
    }

    file.flush().await?;

    // Validate file isn't empty
    if downloaded == 0 {
        let _ = tokio::fs::remove_file(file_path).await;
        return Err("Downloaded file is empty".into());
    }

    Ok(())
}

fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tauri::command]
async fn retry_failed_downloads(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<i32, String> {
    let count = state.db.retry_failed_downloads().map_err(|e| e.to_string())?;

    // Re-queue the retried downloads for actual download
    if count > 0 {
        let downloads = state.db.get_downloads().map_err(|e| e.to_string())?;
        for dl in downloads.iter().filter(|d| d.status == "queued") {
            let _ = app_handle.emit("download-retry", serde_json::json!({ "id": dl.id }));
        }
    }

    Ok(count)
}

#[tauri::command]
async fn delete_download(
    state: tauri::State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), String> {
    state.db.delete_download(id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn clear_completed_downloads(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.db.clear_completed_downloads().map_err(|e| e.to_string())
}

// --- Mokuroku (Watchlist) commands ---

#[tauri::command]
async fn add_to_mokuroku(
    state: tauri::State<'_, Arc<AppState>>,
    anime_id: i64,
    title: String,
    cover_image: Option<String>,
) -> Result<(), String> {
    state
        .db
        .add_to_mokuroku(anime_id, &title, cover_image.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_from_mokuroku(
    state: tauri::State<'_, Arc<AppState>>,
    anime_id: i64,
) -> Result<(), String> {
    state
        .db
        .remove_from_mokuroku(anime_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_mokuroku(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<MokurokuEntry>, String> {
    state.db.get_mokuroku().map_err(|e| e.to_string())
}

#[tauri::command]
async fn is_in_mokuroku(
    state: tauri::State<'_, Arc<AppState>>,
    anime_id: i64,
) -> Result<bool, String> {
    state
        .db
        .is_in_mokuroku(anime_id)
        .map_err(|e| e.to_string())
}

// --- Settings commands ---

#[tauri::command]
async fn get_settings(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let mut settings = std::collections::HashMap::new();
    let allow_adult = state
        .db
        .get_setting("allow_adult")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "false".to_string());
    settings.insert("allow_adult".to_string(), allow_adult);
    let disclaimer = state
        .db
        .get_setting("disclaimer_accepted")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "false".to_string());
    settings.insert("disclaimer_accepted".to_string(), disclaimer);
    Ok(settings)
}

#[tauri::command]
async fn set_setting(
    state: tauri::State<'_, Arc<AppState>>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.db.set_setting(&key, &value).map_err(|e| e.to_string())
}

// Simple adult content toggle — no auth, just confirmation from frontend
#[tauri::command]
async fn toggle_adult_content(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    let current = state.allow_adult();
    let new_value = if current { "false" } else { "true" };
    state
        .db
        .set_setting("allow_adult", new_value)
        .map_err(|e| e.to_string())?;
    Ok(!current)
}

// --- Quit command ---

#[tauri::command]
async fn quit_app(app_handle: tauri::AppHandle) -> Result<(), String> {
    app_handle.exit(0);
    Ok(())
}

// --- mpv fallback command ---

#[tauri::command]
async fn play_in_mpv(
    url: String,
    title: String,
    referrer: Option<String>,
) -> Result<(), String> {
    let mut cmd = std::process::Command::new("mpv");
    cmd.arg(&url);
    cmd.arg(format!("--force-media-title={}", title));

    if let Some(refr) = referrer {
        cmd.arg(format!("--referrer={}", refr));
    }

    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to launch mpv: {}", e))?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));

            let state = Arc::new(AppState {
                anilist: AniListClient::new(),
                allanime: AllAnimeClient::new(),
                db: Database::new(&data_dir).expect("Failed to initialize database"),
                download_semaphore: Arc::new(tokio::sync::Semaphore::new(3)),
            });

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            search_anime,
            get_trending,
            get_recently_updated,
            get_anime_detail,
            browse_genre,
            provider_search,
            get_episodes,
            get_episode_sources,
            add_to_history,
            get_history,
            get_continue_watching,
            get_watched_episodes,
            get_downloads,
            check_local_file,
            start_download,
            retry_failed_downloads,
            delete_download,
            clear_completed_downloads,
            add_to_mokuroku,
            remove_from_mokuroku,
            get_mokuroku,
            is_in_mokuroku,
            get_settings,
            set_setting,
            toggle_adult_content,
            play_in_mpv,
            quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running shizuku");
}
