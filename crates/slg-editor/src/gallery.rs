//! 地图画廊后端

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slg_data::map_doc::*;
use slg_save::container::*;
use std::path::{Path, PathBuf};

/// 地图画廊条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapGalleryEntry {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub preview_path: Option<PathBuf>,
    pub tags: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub author: String,
}

/// 地图画廊
#[derive(Resource, Default)]
pub struct MapGallery {
    pub entries: Vec<MapGalleryEntry>,
    pub selected_index: Option<usize>,
    pub filter_tag: Option<String>,
}

impl MapGallery {
    /// 扫描目录加载地图
    pub fn scan_directory(&mut self, dir: &Path) {
        if !dir.exists() {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "slgmap") {
                    if let Ok(doc) = load_map_from_file(&path) {
                        let gallery_entry = MapGalleryEntry {
                            name: doc.meta.name.clone(),
                            description: format!("{}x{}", doc.meta.width, doc.meta.height),
                            path: path.clone(),
                            preview_path: Some(path.with_extension("png")),
                            tags: Vec::new(),
                            width: doc.meta.width,
                            height: doc.meta.height,
                            author: "未知".to_string(),
                        };
                        self.entries.push(gallery_entry);
                    }
                }
            }
        }
    }

    /// 按标签筛选
    pub fn filter_by_tag(&mut self, tag: Option<String>) {
        self.filter_tag = tag;
    }

    /// 获取筛选后的条目
    pub fn filtered_entries(&self) -> Vec<&MapGalleryEntry> {
        match &self.filter_tag {
            Some(tag) => self
                .entries
                .iter()
                .filter(|e| e.tags.contains(tag))
                .collect(),
            None => self.entries.iter().collect(),
        }
    }

    /// 选中地图
    pub fn select(&mut self, index: usize) {
        self.selected_index = Some(index);
    }

    /// 获取选中的地图
    pub fn selected(&self) -> Option<&MapGalleryEntry> {
        self.selected_index.and_then(|i| self.entries.get(i))
    }

    /// 加载选中的地图
    pub fn load_selected(&self) -> Option<MapDocument> {
        self.selected()
            .and_then(|entry| load_map_from_file(&entry.path).ok())
    }
}

/// 扫描内置地图目录
pub fn scan_builtin_maps(gallery: &mut MapGallery) {
    let builtin_dir = Path::new("assets/maps");
    gallery.scan_directory(builtin_dir);
}

/// 扫描用户地图目录
pub fn scan_user_maps(gallery: &mut MapGallery) {
    let user_dir = Path::new("user/maps");
    gallery.scan_directory(user_dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gallery_default() {
        let gallery = MapGallery::default();
        assert!(gallery.entries.is_empty());
        assert!(gallery.selected().is_none());
    }

    #[test]
    fn test_gallery_filter() {
        let mut gallery = MapGallery::default();
        gallery.entries.push(MapGalleryEntry {
            name: "测试1".to_string(),
            description: "".to_string(),
            path: PathBuf::new(),
            preview_path: None,
            tags: vec!["标准".to_string()],
            width: 256,
            height: 256,
            author: "".to_string(),
        });
        gallery.entries.push(MapGalleryEntry {
            name: "测试2".to_string(),
            description: "".to_string(),
            path: PathBuf::new(),
            preview_path: None,
            tags: vec!["挑战".to_string()],
            width: 256,
            height: 256,
            author: "".to_string(),
        });

        gallery.filter_by_tag(Some("标准".to_string()));
        let filtered = gallery.filtered_entries();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "测试1");
    }

    #[test]
    fn test_gallery_select() {
        let mut gallery = MapGallery::default();
        gallery.entries.push(MapGalleryEntry {
            name: "测试".to_string(),
            description: "".to_string(),
            path: PathBuf::new(),
            preview_path: None,
            tags: vec![],
            width: 256,
            height: 256,
            author: "".to_string(),
        });

        gallery.select(0);
        assert!(gallery.selected().is_some());
        assert_eq!(gallery.selected().unwrap().name, "测试");
    }
}
