use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkNode {
    #[serde(default)]
    pub date_added: String,
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String, // "url" or "folder"
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub children: Option<Vec<BookmarkNode>>,
}

fn default_bookmark_bar() -> BookmarkNode {
    BookmarkNode {
        date_added: "0".to_string(),
        id: "1".to_string(),
        name: "Bookmarks bar".to_string(),
        node_type: "folder".to_string(),
        url: None,
        children: Some(Vec::new()),
    }
}

fn default_other() -> BookmarkNode {
    BookmarkNode {
        date_added: "0".to_string(),
        id: "2".to_string(),
        name: "Other bookmarks".to_string(),
        node_type: "folder".to_string(),
        url: None,
        children: Some(Vec::new()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkRoots {
    #[serde(default = "default_bookmark_bar")]
    pub bookmark_bar: BookmarkNode,
    #[serde(default = "default_other")]
    pub other: BookmarkNode,
}

fn default_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromiumBookmarks {
    pub roots: BookmarkRoots,
    #[serde(default = "default_version")]
    pub version: u32,
}

impl Default for ChromiumBookmarks {
    fn default() -> Self {
        Self {
            roots: BookmarkRoots {
                bookmark_bar: default_bookmark_bar(),
                other: default_other(),
            },
            version: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BookmarkItem {
    pub uri: String,
    pub title: String,
    pub folder: String,
}

#[derive(Debug)]
pub struct BookmarkStore {
    path: PathBuf,
    data: ChromiumBookmarks,
}

impl BookmarkStore {
    pub fn new(custom_path: Option<PathBuf>) -> Self {
        let path = custom_path.unwrap_or_else(|| {
            let dirs = directories::ProjectDirs::from("org", "neoweb", "neoweb");
            let config_dir = dirs
                .map(|d| d.config_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            config_dir.join("Bookmarks")
        });

        let data = if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match serde_json::from_str::<ChromiumBookmarks>(&content) {
                    Ok(parsed) => {
                        info!("Loaded Chromium bookmarks from {:?}", path);
                        parsed
                    }
                    Err(e) => {
                        error!("Failed to parse Chromium bookmarks at {:?}: {}", path, e);
                        ChromiumBookmarks::default()
                    }
                },
                Err(e) => {
                    error!("Failed to read bookmarks file at {:?}: {}", path, e);
                    ChromiumBookmarks::default()
                }
            }
        } else {
            let def = ChromiumBookmarks::default();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(&def) {
                let _ = fs::write(&path, json);
            }
            def
        };

        Self { path, data }
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn add_bookmark(
        &mut self,
        url: &str,
        title: &str,
        target_folder: Option<&str>,
    ) -> Result<(), std::io::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_string();

        let new_node = BookmarkNode {
            date_added: now.clone(),
            id: now,
            name: if title.is_empty() {
                url.to_string()
            } else {
                title.to_string()
            },
            node_type: "url".to_string(),
            url: Some(url.to_string()),
            children: None,
        };

        let is_other = target_folder
            .map(|f| f.eq_ignore_ascii_case("other"))
            .unwrap_or(false);
        if is_other {
            let children = self.data.roots.other.children.get_or_insert_with(Vec::new);
            children.push(new_node);
        } else {
            let children = self
                .data
                .roots
                .bookmark_bar
                .children
                .get_or_insert_with(Vec::new);
            children.push(new_node);
        }

        self.save()
    }

    pub fn search(&self, query: &str) -> Vec<BookmarkItem> {
        let mut results = Vec::new();
        let q = query.to_lowercase();

        Self::collect_nodes(
            &self.data.roots.bookmark_bar,
            "Bookmarks bar",
            &q,
            &mut results,
        );
        Self::collect_nodes(&self.data.roots.other, "Other bookmarks", &q, &mut results);

        results
    }

    fn collect_nodes(
        node: &BookmarkNode,
        current_folder: &str,
        query: &str,
        acc: &mut Vec<BookmarkItem>,
    ) {
        if node.node_type == "url"
            && let Some(url) = &node.url
        {
            let matches = query.is_empty()
                || node.name.to_lowercase().contains(query)
                || url.to_lowercase().contains(query);
            if matches {
                acc.push(BookmarkItem {
                    uri: url.clone(),
                    title: node.name.clone(),
                    folder: current_folder.to_string(),
                });
            }
        }

        if let Some(children) = &node.children {
            let folder_name = if node.node_type == "folder" {
                node.name.as_str()
            } else {
                current_folder
            };
            for child in children {
                Self::collect_nodes(child, folder_name, query, acc);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chromium_bookmarks_store() {
        let temp_dir = std::env::temp_dir().join(format!("neoweb_test_bm_{}", std::process::id()));
        let bm_path = temp_dir.join("Bookmarks");

        let mut store = BookmarkStore::new(Some(bm_path.clone()));
        assert_eq!(store.search("").len(), 0);

        store
            .add_bookmark("https://servo.org", "Servo Project", None)
            .unwrap();
        store
            .add_bookmark("https://crates.io", "Rust Crates", Some("other"))
            .unwrap();

        let all = store.search("");
        assert_eq!(all.len(), 2);

        let servo_match = store.search("servo");
        assert_eq!(servo_match.len(), 1);
        assert_eq!(servo_match[0].uri, "https://servo.org");
        assert_eq!(servo_match[0].title, "Servo Project");

        // Reload from file to verify persistence
        let reloaded = BookmarkStore::new(Some(bm_path));
        let reloaded_all = reloaded.search("");
        assert_eq!(reloaded_all.len(), 2);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
