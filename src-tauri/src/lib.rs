use std::fs;
use std::path::PathBuf;
use tauri::Emitter;

fn get_config_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("fitgirl-downloader");
    let _ = fs::create_dir_all(&path);
    path.push("config.json");
    path
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
struct SavedLink {
    name: String,
    url: String,
    downloaded: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct AppConfig {
    browser_path: Option<String>,
    download_dir: Option<String>,
    links: Option<Vec<SavedLink>>,
}

#[tauri::command]
fn load_config() -> Result<AppConfig, String> {
    let path = get_config_path();
    if path.exists() {
        let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let config: AppConfig = serde_json::from_str(&contents).unwrap_or_default();
        Ok(config)
    } else {
        Ok(AppConfig::default())
    }
}

#[tauri::command]
fn save_config(browser_path: Option<String>, download_dir: Option<String>) -> Result<(), String> {
    let path = get_config_path();
    // Preserve existing links when saving config
    let existing_links = if path.exists() {
        fs::read_to_string(&path)
            .ok()
            .and_then(|c| serde_json::from_str::<AppConfig>(&c).ok())
            .and_then(|c| c.links)
    } else {
        None
    };
    let config = AppConfig { browser_path, download_dir, links: existing_links };
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn save_links(links: Vec<SavedLink>) -> Result<(), String> {
    let path = get_config_path();
    let (browser_path, download_dir) = if path.exists() {
        if let Some(c) = fs::read_to_string(&path).ok().and_then(|c| serde_json::from_str::<AppConfig>(&c).ok()) {
            (c.browser_path, c.download_dir)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };
    let config = AppConfig { browser_path, download_dir, links: Some(links) };
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn load_links() -> Result<Vec<SavedLink>, String> {
    let path = get_config_path();
    if path.exists() {
        let contents = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let config: AppConfig = serde_json::from_str(&contents).unwrap_or_default();
        Ok(config.links.unwrap_or_default())
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
fn mark_downloaded(url: String) -> Result<(), String> {
    let path = get_config_path();
    if path.exists() {
        let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let mut config: AppConfig = serde_json::from_str(&contents).unwrap_or_default();
        if let Some(ref mut links) = config.links {
            for link in links.iter_mut() {
                if link.url == url {
                    link.downloaded = true;
                    break;
                }
            }
        }
        let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Inisialisasi browser Chromium dengan opsi standar
fn create_browser(
    browser_path: &Option<String>,
    headless: bool,
) -> Result<headless_chrome::Browser, String> {
    let mut builder = headless_chrome::LaunchOptions::default_builder();
    builder.headless(headless).args(vec![
        std::ffi::OsStr::new("--blink-settings=imagesEnabled=false"),
        std::ffi::OsStr::new("--disable-gpu"),
        std::ffi::OsStr::new("--disable-software-rasterizer"),
        std::ffi::OsStr::new("--mute-audio"),
        std::ffi::OsStr::new("--disable-dev-shm-usage"),
        std::ffi::OsStr::new("--disable-extensions"),
        std::ffi::OsStr::new("--disable-background-networking"),
        std::ffi::OsStr::new("--disable-background-timer-throttling"),
        std::ffi::OsStr::new("--disable-backgrounding-occluded-windows"),
        std::ffi::OsStr::new("--disable-breakpad"),
        std::ffi::OsStr::new("--no-sandbox"),
        std::ffi::OsStr::new("--disable-setuid-sandbox"),
    ]);

    if let Some(path) = browser_path {
        if !path.trim().is_empty() {
            builder.path(Some(std::path::PathBuf::from(path.trim())));
        }
    }

    let options = builder
        .build()
        .map_err(|e| format!("Failed to create launch options: {}", e))?;

    headless_chrome::Browser::new(options)
        .map_err(|e| format!("Failed to launch browser: {}", e))
}

/// Polling evaluasi JS setiap 1 detik hingga `max_seconds`.
/// Mengembalikan `true` jika JS mengembalikan nilai truthy.
fn poll_js_bool(
    tab: &std::sync::Arc<headless_chrome::Tab>,
    js: &str,
    max_seconds: u64,
) -> bool {
    for _ in 0..max_seconds {
        if let Ok(res) = tab.evaluate(js, false) {
            if res
                .value
                .unwrap_or_else(|| serde_json::json!(false))
                .as_bool()
                .unwrap_or(false)
            {

                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    false
}

/// Polling string dari `window.VAR` hingga `max_seconds`
fn poll_js_string(
    tab: &std::sync::Arc<headless_chrome::Tab>,
    js: &str,
    max_seconds: u64,
) -> Option<String> {
    for _ in 0..max_seconds {
        if let Ok(res) = tab.evaluate(js, false) {
            if let Some(val) = res.value {
                let s = val.as_str().unwrap_or("").to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    None
}

// ================================
//  SCRAPE LINKS (halaman FitGirl)
// ================================
#[tauri::command]
async fn scrape_links(url: String, browser_path: Option<String>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let browser = create_browser(&browser_path, true)?;
        let tab = browser
            .new_tab()
            .map_err(|e| format!("Failed to open new tab: {}", e))?;
        tab.navigate_to(&url)
            .map_err(|e| format!("Failed to navigate: {}", e))?;

        tab.wait_for_element(".su-spoiler-content.su-u-clearfix.su-u-trim")
            .map_err(|e| format!("Gagal menemukan elemen parent target: {}", e))?;

        let js = r#"
            (function() {
                var parent = document.querySelector('.su-spoiler-content.su-u-clearfix.su-u-trim');
                if (!parent) return JSON.stringify([]);
                var links = parent.querySelectorAll('a');
                var result = [];
                for (var i = 0; i < links.length; i++) {
                    result.push({
                        href: links[i].href,
                        text: links[i].innerText || links[i].textContent
                    });
                }
                return JSON.stringify(result);
            })()
        "#;

        let res = tab
            .evaluate(js, false)
            .map_err(|e| format!("Gagal mengevaluasi ekstrak JS: {}", e))?;

        if let Some(val) = res.value {
            Ok(val.as_str().unwrap_or("[]").to_string())
        } else {
            Ok("[]".to_string())
        }
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

// ================================
//  PROCESS LINK (Datanodes auto-download)
// ================================
#[tauri::command]
async fn process_link(app_handle: tauri::AppHandle, url: String, browser_path: Option<String>, download_dir: Option<String>) -> Result<String, String> {
    let app_handle_spawn = app_handle.clone();
    tokio::task::spawn_blocking(move || {
        let app_handle = app_handle_spawn;
        // Set false saat development agar kelihatan di layar, true saat produksi
        let is_headless = true;

        let browser = create_browser(&browser_path, is_headless)?;
        let tab = browser
            .new_tab()
            .map_err(|e| format!("Failed to open new tab: {}", e))?;


        tab.navigate_to(&url)
            .map_err(|e| format!("Failed to navigate: {}", e))?;

        // ─────────────────────────────────────────────────────
        // TAHAP 1: Klik tombol "Continue to Download" (#method_free)
        // Gunakan form.submit() agar iklan popup tidak bisa mengintervensi
        // ─────────────────────────────────────────────────────


        let js_step1 = r#"
            (function() {
                // Blokir popup tab baru total
                window.open = function() { return null; };

                var btn = document.querySelector('#method_free')
                       || document.querySelector('button[name="method_free"]')
                       || document.querySelector('input[name="method_free"]');

                if (!btn) {
                    // Fallback: cari berdasarkan teks
                    var all = document.querySelectorAll('button, input[type="submit"], a');
                    for (var i = 0; i < all.length; i++) {
                        var t = (all[i].value || all[i].innerText || '').toLowerCase();
                        if (t.includes('continue to download') || t.includes('free download')) {
                            btn = all[i]; break;
                        }
                    }
                }

                if (!btn || btn.offsetParent === null) return false;

                // Submit parent form langsung — 100% bypass ad overlay
                var form = btn.closest('form');
                if (form) {
                    var inp = document.createElement('input');
                    inp.type  = 'hidden';
                    inp.name  = btn.name  || 'method_free';
                    inp.value = btn.value || 'Free Download >>';
                    form.appendChild(inp);
                    form.submit();
                    return true;
                }

                btn.click();
                return true;
            })();
        "#;

        if !poll_js_bool(&tab, js_step1, 30) {
            return Err("Gagal klik tombol 'Continue to Download'. Mungkin Cloudflare memblokir.".to_string());
        }

        // Tunggu halaman Datanodes load setelah form submit

        std::thread::sleep(std::time::Duration::from_secs(4));

        // ─────────────────────────────────────────────────────
        // INJECT FETCH/XHR HOOK sebelum klik apapun di halaman Datanodes
        // Ini HARUS dilakukan sebelum tombol Download diklik agar interceptor siap
        // ─────────────────────────────────────────────────────

        let js_hook = r#"
            (function() {
                if (window._fitgirlHooked) return;
                window._fitgirlHooked = true;
                window._finalUrl = '';

                // Hook modern Fetch API
                var _origFetch = window.fetch;
                window.fetch = async function() {
                    var res = await _origFetch.apply(this, arguments);
                    var clone = res.clone();
                    try {
                        var text = await clone.text();
                        var obj  = JSON.parse(text);
                        if (obj && obj.url && obj.url.length > 10) {
                            window._finalUrl = decodeURIComponent(obj.url);
                            // Kembalikan response kosong agar Vue tidak membuka popup browser
                            obj.url = '';
                            return new Response(JSON.stringify(obj), {
                                status: res.status, statusText: res.statusText, headers: res.headers
                            });
                        }
                    } catch(e) {}
                    return res;
                };

                // Hook XHR klasik sebagai fallback
                var _origOpen = XMLHttpRequest.prototype.open;
                XMLHttpRequest.prototype.open = function() {
                    this.addEventListener('load', function() {
                        try {
                            var obj = JSON.parse(this.responseText);
                            if (obj && obj.url && obj.url.length > 10) {
                                window._finalUrl = decodeURIComponent(obj.url);
                            }
                        } catch(e) {}
                    });
                    _origOpen.apply(this, arguments);
                };
            })();
        "#;

        tab.evaluate(js_hook, false)
            .map_err(|e| format!("Gagal inject hook: {}", e))?;

        // ─────────────────────────────────────────────────────
        // TAHAP 2: Cari & klik tombol "Download" (button.py-3)
        // Abaikan jika disabled (timer Vue countdown), tunggu sampai aktif
        // ─────────────────────────────────────────────────────


        let js_step2 = r#"
            (function() {
                window.open = function() { return null; };

                // Cari spesifik button.py-3 saja, sesuai HTML Datanodes
                var btn = null;
                var candidates = document.querySelectorAll('button.py-3, a.py-3');
                for (var i = 0; i < candidates.length; i++) {
                    var c = candidates[i];
                    var txt = (c.innerText || '').trim().toLowerCase();
                    // Skip jika teks mengandung register/login/premium
                    if (txt.includes('register') || txt.includes('login') || txt.includes('premium')) continue;
                    // Skip jika tombol masih disabled (timer countdown)
                    if (c.disabled || c.getAttribute('disabled') !== null) continue;
                    // Skip jika tidak terlihat
                    if (c.offsetParent === null) continue;
                    // Cocokkan teks "download" (bukan "continue" dulu)
                    if (txt === 'download' || txt.includes('download')) {
                        btn = c; break;
                    }
                }

                if (!btn) return false;

                // Klik dua kali karena Datanodes mensyaratkan dua klik untuk memicu countdown
                btn.dispatchEvent(new MouseEvent('mousedown', {bubbles:true, cancelable:true, view:window}));
                btn.dispatchEvent(new MouseEvent('mouseup',   {bubbles:true, cancelable:true, view:window}));
                btn.dispatchEvent(new MouseEvent('click',     {bubbles:true, cancelable:true, view:window}));
                btn.click();
                return true;
            })();
        "#;

        if !poll_js_bool(&tab, js_step2, 30) {
            return Err("Gagal klik tombol Download (py-3). Halaman belum siap atau tombol tidak ditemukan.".to_string());
        }

        // Tunggu timer countdown Datanodes (~6 detik) + sedikit buffer

        std::thread::sleep(std::time::Duration::from_secs(8));

        // ─────────────────────────────────────────────────────
        // TAHAP 3: Setelah timer selesai, tombol berubah menjadi "Continue"
        // Klik — fetch hook akan menangkap URL download dari response JSON
        // ─────────────────────────────────────────────────────


        let js_step3 = r#"
            (function() {
                window.open = function() { return null; };

                var btn = null;
                var candidates = document.querySelectorAll('button.py-3, a.py-3');
                for (var i = 0; i < candidates.length; i++) {
                    var c = candidates[i];
                    var txt = (c.innerText || '').trim().toLowerCase();
                    if (txt.includes('register') || txt.includes('login') || txt.includes('premium')) continue;
                    if (c.disabled || c.getAttribute('disabled') !== null) continue;
                    if (c.offsetParent === null) continue;
                    // Sekarang kita cari "continue" ATAU "download" (tombol bisa teks keduanya)
                    if (txt === 'continue' || txt.includes('continue') || txt === 'download' || txt.includes('download')) {
                        btn = c; break;
                    }
                }

                if (!btn) return false;

                // Jika link langsung (<a href="...">), ambil href tanpa klik
                if (btn.tagName && btn.tagName.toLowerCase() === 'a'
                    && btn.href && btn.href.startsWith('http')
                    && !btn.href.startsWith(window.location.origin)) {
                    window._finalUrl = btn.href;
                    return true;
                }

                // Klik organik untuk memicu fetch() di Vue
                btn.dispatchEvent(new MouseEvent('mousedown', {bubbles:true, cancelable:true, view:window}));
                btn.dispatchEvent(new MouseEvent('mouseup',   {bubbles:true, cancelable:true, view:window}));
                btn.dispatchEvent(new MouseEvent('click',     {bubbles:true, cancelable:true, view:window}));
                btn.click();

                return true;
            })();
        "#;

        // Poll hingga 30 detik (timer countdown bisa lebih lama dari 8 detik kadang)
        if !poll_js_bool(&tab, js_step3, 30) {
            return Err("Gagal klik tombol Continue. Timer mungkin belum selesai atau tombol tidak muncul.".to_string());
        }

        // ─────────────────────────────────────────────────────
        // Tunggu URL dari hook (fetch/XHR mencuri URL download asli)
        // ─────────────────────────────────────────────────────

        let final_url = match poll_js_string(&tab, "window._finalUrl || '';", 30) {
            Some(u) if !u.is_empty() => u,
            _ => {
                // Cek apakah redirect ke premium
                if let Ok(res) = tab.evaluate("window.location.href", false) {
                    if let Some(v) = res.value {
                        if v.as_str().unwrap_or("").contains("/premium") {
                            return Err("Limit harian habis — Datanodes mengarahkan ke halaman premium.".to_string());
                        }
                    }
                }
                return Err("Fetch hook tidak menangkap URL download. Coba periksa apakah timer Datanodes sudah selesai.".to_string());
            }
        };

        // Ambil cookies browser untuk dikirim bersama request reqwest
        let mut cookie_str = String::new();
        if let Ok(res) = tab.evaluate("document.cookie", false) {
            if let Some(v) = res.value {
                cookie_str = v.as_str().unwrap_or("").to_string();
            }
        }



        // ─────────────────────────────────────────────────────
        // Download via reqwest — stream to disk + emit real-time progress events
        // ─────────────────────────────────────────────────────
        let ori_url    = url.clone();
        let dl_url     = final_url.clone();
        let cookie_hdr = cookie_str.clone();
        let app_handle = app_handle.clone();
        let download_dir = download_dir.clone();

        tokio::spawn(async move {
            let dl_dir = if let Some(dir) = download_dir {
                if dir.trim().is_empty() {
                    dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("Fitgirl_Downloads")
                } else {
                    std::path::PathBuf::from(dir.trim())
                }
            } else {
                dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join("Fitgirl_Downloads")
            };
            let _ = std::fs::create_dir_all(&dl_dir);

            let client = reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/120.0 Safari/537.36")
                .build()
                .unwrap_or_else(|_| reqwest::Client::new());

            let mut req = client.get(&dl_url);
            if !cookie_hdr.is_empty() {
                req = req.header(reqwest::header::COOKIE, &cookie_hdr);
            }



            let response = match req.send().await {
                Ok(r)  => r,
                Err(e) => {

                    let _ = app_handle.emit("download-progress", serde_json::json!({
                        "status": "error", "message": e.to_string()
                    }));
                    return;
                }
            };

            let total_size = response.content_length();
            let total_mb   = total_size.map(|t| t as f64 / 1_048_576.0).unwrap_or(0.0);

            // Tentukan nama file
            let mut file_name = String::new();
            if let Some(cd) = response.headers().get(reqwest::header::CONTENT_DISPOSITION) {
                if let Ok(cd_str) = cd.to_str() {
                    if let Some(pos) = cd_str.find("filename=") {
                        let raw = &cd_str[pos + 9..];
                        file_name = raw.trim_matches(|c| c == '"' || c == '\'' || c == ';')
                                       .split(';').next().unwrap_or("").trim().to_string();
                    }
                }
            }
            if file_name.is_empty() {
                let raw   = ori_url.split('/').last().unwrap_or("fitgirl_download.rar");
                let clean = raw.split('?').next().unwrap_or("fitgirl_download.rar");
                file_name = clean
                    .replace("%20", "_").replace("%28", "(")
                    .replace("%29", ")").replace("%5B", "[").replace("%5D", "]");
            }
            if file_name.is_empty() { file_name = "fitgirl_download.rar".to_string(); }

            // Sanitasi nama file agar aman di Windows (menghindari os error 123)
            file_name = file_name.replace(|c| ['<', '>', ':', '"', '/', '\\', '|', '?', '*'].contains(&c), "_");

            let dest = dl_dir.join(&file_name);


            let mut file = match std::fs::File::create(&dest) {
                Ok(f)  => f,
                Err(e) => {

                    let _ = app_handle.emit("download-progress", serde_json::json!({
                        "status": "error", "message": e.to_string()
                    }));
                    return;
                }
            };

            // Emit start event
            let _ = app_handle.emit("download-progress", serde_json::json!({
                "status": "started",
                "filename": file_name,
                "total_mb": total_mb,
                "downloaded_mb": 0.0,
                "percent": 0.0,
                "speed_kbps": 0.0,
            }));

            use std::io::Write;
            let mut downloaded: u64 = 0;
            let mut resp = response;

            // Speed tracking: bytes downloaded within the last second interval
            let mut interval_bytes: u64 = 0;
            let mut last_emit = std::time::Instant::now();

            while let Ok(Some(chunk)) = resp.chunk().await {
                let _ = file.write_all(&chunk);
                downloaded     += chunk.len() as u64;
                interval_bytes += chunk.len() as u64;

                // Emit progress roughly every 500ms
                let elapsed = last_emit.elapsed();
                if elapsed.as_millis() >= 500 {
                    let secs        = elapsed.as_secs_f64().max(0.001);
                    let speed_kbps  = (interval_bytes as f64 / 1024.0) / secs;
                    let dl_mb       = downloaded as f64 / 1_048_576.0;
                    let percent     = if total_mb > 0.0 { dl_mb / total_mb * 100.0 } else { 0.0 };

                    let _ = app_handle.emit("download-progress", serde_json::json!({
                        "status": "progress",
                        "filename": file_name,
                        "total_mb": total_mb,
                        "downloaded_mb": dl_mb,
                        "percent": percent,
                        "speed_kbps": speed_kbps,
                    }));



                    interval_bytes = 0;
                    last_emit      = std::time::Instant::now();
                }
            }

            let dl_mb = downloaded as f64 / 1_048_576.0;


            let _ = app_handle.emit("download-progress", serde_json::json!({
                "status": "done",
                "filename": file_name,
                "total_mb": dl_mb,
                "downloaded_mb": dl_mb,
                "percent": 100.0,
                "speed_kbps": 0.0,
            }));
        });

        Ok(format!(
            "✅ Download dimulai!\nFile akan tersimpan di ~/Downloads/Fitgirl_Downloads/\nAnda bisa menutup aplikasi kapan saja."
        ))
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            scrape_links, load_config, save_config,
            save_links, load_links, mark_downloaded,
            process_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
