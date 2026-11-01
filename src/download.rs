//! File download delegation, background download manager, and filename sanitization.

use percent_encoding::percent_decode_str;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tracing::{error, info};
use url::Url;

/// Known downloadable file extensions to intercept during page navigation.
pub const DOWNLOAD_EXTENSIONS: &[&str] = &[
    // Archives & Compressed formats
    "zip", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "7z", "rar", "zst",
    // Binaries, Executables & OS packages
    "iso", "img", "bin", "exe", "msi", "deb", "rpm", "apk", "dmg", "pkg", "appimage",
    // Documents & Torrents commonly downloaded
    "torrent",
];

/// Checks if a navigation URL points to a downloadable file based on its path extension.
pub fn is_downloadable_url(url: &Url) -> bool {
    let path = url.path();
    let ext = match path.rsplit('.').next() {
        Some(e) if !e.is_empty() && e.len() <= 10 => e.to_ascii_lowercase(),
        _ => return false,
    };

    DOWNLOAD_EXTENSIONS.contains(&ext.as_str())
}

/// Derives a clean, safe filename from a URL or falls back to `"download"`.
pub fn suggest_filename_from_url(url: &Url) -> String {
    let raw_name = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|s| !s.is_empty()))
        .unwrap_or("download");

    let decoded = percent_decode_str(raw_name)
        .decode_utf8()
        .unwrap_or(std::borrow::Cow::Borrowed("download"));

    // Sanitize: strip directories, path separators, and null bytes
    let sanitized: String = decoded
        .chars()
        .filter(|&c| c != '/' && c != '\\' && c != '\0' && !c.is_control())
        .collect();

    let trimmed = sanitized.trim_matches(|c| c == '.' || c == ' ');
    if trimmed.is_empty() {
        "download".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Resolves the default download directory (XDG Downloads, ~/Downloads, or current directory).
pub fn get_default_download_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| {
            std::env::var("XDG_DOWNLOAD_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    std::env::var("HOME")
                        .map(|h| PathBuf::from(h).join("Downloads"))
                        .unwrap_or_else(|_| {
                            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                        })
                })
        })
}

/// Generates a unique filepath in `dir` avoiding collisions (e.g. `file (1).ext`).
pub fn generate_unique_filepath(dir: &Path, filename: &str) -> PathBuf {
    let target = dir.join(filename);
    if !target.exists() {
        return target;
    }

    let (stem, ext) = match filename.rfind('.') {
        Some(dot) if dot > 0 => (&filename[..dot], &filename[dot..]),
        _ => (filename, ""),
    };

    let mut counter = 1;
    loop {
        let candidate = dir.join(format!("{stem} ({counter}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

/// Formats byte sizes into human-readable strings (e.g. `12.5 MB`).
pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;

    if bytes >= GIB {
        format!("{:.2} GB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Events emitted during the lifecycle of a file download.
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Started {
        id: u64,
        url: String,
        filename: String,
        path: PathBuf,
    },
    Progress {
        id: u64,
        downloaded: u64,
        total: Option<u64>,
    },
    Finished {
        id: u64,
        filename: String,
        path: PathBuf,
        size: u64,
    },
    Failed {
        id: u64,
        filename: String,
        error: String,
    },
}

static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

static DOWNLOAD_EVENT_TX: LazyLock<Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<DownloadEvent>>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

pub fn register_download_listener(tx: tokio::sync::mpsc::UnboundedSender<DownloadEvent>) {
    if let Ok(mut guard) = DOWNLOAD_EVENT_TX.lock() {
        *guard = Some(tx);
    }
}

pub fn send_download_event(ev: DownloadEvent) {
    if let Ok(guard) = DOWNLOAD_EVENT_TX.lock()
        && let Some(tx) = guard.as_ref()
    {
        let _ = tx.send(ev);
    }
}

/// Initiates an asynchronous background download of the specified URL.
pub fn start_download(url_str: String, custom_destination: Option<PathBuf>) -> u64 {
    let id = NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed);

    let parsed_url = match Url::parse(&url_str) {
        Ok(u) => u,
        Err(e) => {
            let filename = "download".to_string();
            send_download_event(DownloadEvent::Failed {
                id,
                filename,
                error: format!("Invalid URL: {e}"),
            });
            return id;
        }
    };

    let filename = suggest_filename_from_url(&parsed_url);

    let final_path = match custom_destination {
        Some(custom) => {
            if custom.is_dir() {
                generate_unique_filepath(&custom, &filename)
            } else {
                custom
            }
        }
        None => {
            let default_dir = get_default_download_dir();
            generate_unique_filepath(&default_dir, &filename)
        }
    };

    info!(
        id,
        url = %url_str,
        path = ?final_path,
        "Initiating download"
    );

    send_download_event(DownloadEvent::Started {
        id,
        url: url_str.clone(),
        filename: filename.clone(),
        path: final_path.clone(),
    });

    let target_path = final_path.clone();
    let file_name_clone = filename.clone();

    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let res = match client.get(&url_str).send().await {
            Ok(r) => r,
            Err(e) => {
                error!(id, url = %url_str, "Download request failed: {e}");
                send_download_event(DownloadEvent::Failed {
                    id,
                    filename: file_name_clone,
                    error: e.to_string(),
                });
                return;
            }
        };

        if !res.status().is_success() {
            let status = res.status();
            error!(id, url = %url_str, "Download failed with status: {status}");
            send_download_event(DownloadEvent::Failed {
                id,
                filename: file_name_clone,
                error: format!("HTTP error: {status}"),
            });
            return;
        }

        let total_size = res.content_length();

        if let Some(parent) = target_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        let mut file = match tokio::fs::File::create(&target_path).await {
            Ok(f) => f,
            Err(e) => {
                error!(id, path = ?target_path, "Failed to create destination file: {e}");
                send_download_event(DownloadEvent::Failed {
                    id,
                    filename: file_name_clone,
                    error: format!("I/O create error: {e}"),
                });
                return;
            }
        };

        let mut stream = res;
        let mut downloaded: u64 = 0;
        let mut last_report = Instant::now();

        loop {
            match stream.chunk().await {
                Ok(Some(chunk)) => {
                    if let Err(e) = file.write_all(&chunk).await {
                        error!(id, path = ?target_path, "Failed writing chunk: {e}");
                        send_download_event(DownloadEvent::Failed {
                            id,
                            filename: file_name_clone,
                            error: format!("I/O write error: {e}"),
                        });
                        return;
                    }
                    downloaded += chunk.len() as u64;

                    if last_report.elapsed() >= Duration::from_millis(500) {
                        send_download_event(DownloadEvent::Progress {
                            id,
                            downloaded,
                            total: total_size,
                        });
                        last_report = Instant::now();
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    error!(id, url = %url_str, "Error reading response chunk: {e}");
                    send_download_event(DownloadEvent::Failed {
                        id,
                        filename: file_name_clone,
                        error: format!("Network stream error: {e}"),
                    });
                    return;
                }
            }
        }

        if let Err(e) = file.flush().await {
            error!(id, path = ?target_path, "Failed to flush file: {e}");
            send_download_event(DownloadEvent::Failed {
                id,
                filename: file_name_clone,
                error: format!("I/O flush error: {e}"),
            });
            return;
        }

        info!(
            id,
            filename = %file_name_clone,
            size = downloaded,
            path = ?target_path,
            "Download finished successfully"
        );

        send_download_event(DownloadEvent::Finished {
            id,
            filename: file_name_clone,
            path: target_path,
            size: downloaded,
        });
    });

    id
}

/// Creates an Iced stream producing [`crate::app::Message::Download`] events for the GUI event loop.
pub fn download_event_stream() -> impl iced::futures::Stream<Item = crate::app::Message> {
    use iced::futures::SinkExt;
    iced::stream::channel(
        100,
        |mut output: iced::futures::channel::mpsc::Sender<crate::app::Message>| async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            register_download_listener(tx);
            while let Some(ev) = rx.recv().await {
                let _ = output.send(crate::app::Message::Download(ev)).await;
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_downloadable_url() {
        let zip_url = Url::parse("https://example.com/archive.zip").unwrap();
        assert!(is_downloadable_url(&zip_url));

        let tar_gz_url = Url::parse("https://example.com/release.tar.gz").unwrap();
        assert!(is_downloadable_url(&tar_gz_url));

        let iso_url = Url::parse("https://releases.ubuntu.com/24.04/ubuntu.iso").unwrap();
        assert!(is_downloadable_url(&iso_url));

        let deb_url = Url::parse("https://packages.org/package.deb").unwrap();
        assert!(is_downloadable_url(&deb_url));

        let html_url = Url::parse("https://example.com/index.html").unwrap();
        assert!(!is_downloadable_url(&html_url));

        let page_url = Url::parse("https://example.com/docs/getting-started").unwrap();
        assert!(!is_downloadable_url(&page_url));
    }

    #[test]
    fn test_suggest_filename_from_url() {
        let u1 = Url::parse("https://example.com/files/document%202026.pdf").unwrap();
        assert_eq!(suggest_filename_from_url(&u1), "document 2026.pdf");

        let u2 = Url::parse("https://example.com/release-v1.0.tar.gz").unwrap();
        assert_eq!(suggest_filename_from_url(&u2), "release-v1.0.tar.gz");

        let u3 = Url::parse("https://example.com/").unwrap();
        assert_eq!(suggest_filename_from_url(&u3), "download");

        let u4 = Url::parse("https://example.com/path/../../../malicious.sh").unwrap();
        assert_eq!(suggest_filename_from_url(&u4), "malicious.sh");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[test]
    fn test_generate_unique_filepath() {
        let temp_dir = std::env::temp_dir().join(format!("neoweb_dl_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let f1 = temp_dir.join("test.txt");
        std::fs::write(&f1, b"hello").unwrap();

        let unique = generate_unique_filepath(&temp_dir, "test.txt");
        assert_eq!(unique, temp_dir.join("test (1).txt"));

        std::fs::write(&unique, b"world").unwrap();
        let unique2 = generate_unique_filepath(&temp_dir, "test.txt");
        assert_eq!(unique2, temp_dir.join("test (2).txt"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
