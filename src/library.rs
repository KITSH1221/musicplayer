//! 数据层：一首歌的数据结构，以及把文件系统变成歌单。
//!
//! 这一层完全不碰终端、也不碰音频设备，可以单独测试。

use lofty::prelude::*;
use lofty::read_from_path;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::util::fmt_time;

/// 一首歌。只有 `path` 是必需的，其余信息都可以降级缺失。
pub struct Track {
    path: PathBuf,
    title: String,
    artist: String,
    album: String,
    duration: Option<Duration>,
}

impl Track {
    /// 从一个文件路径构造 Track，尽量读取标签，失败就降级。
    pub fn from_path(path: PathBuf) -> Self {
        // ① 兜底标题 = 文件名。必须在 path 被移进结构体之前算出来。
        let fallback = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();

        let mut track = Track {
            path,
            title: fallback,
            artist: "Unknown Artist".to_string(),
            album: "Unknown Album".to_string(),
            duration: None,
        };

        // ② 读标签。失败不是错误，只是少了额外信息。
        if let Ok(tagged) = read_from_path(&track.path) {
            // duration 来自音频流属性，不是标签
            track.duration = Some(tagged.properties().duration());

            if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
                if let Some(title) = tag.title() {
                    track.title = title.into_owned();
                }
                if let Some(artist) = tag.artist() {
                    track.artist = artist.into_owned();
                }
                if let Some(album) = tag.album() {
                    track.album = album.into_owned();
                }
            }
        }

        track
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 形如 `Radiohead — Jigsaw Falling Into Place  [04:08]`，用于列表
    pub fn display_line(&self) -> String {
        let dur = self
            .duration
            .map(fmt_time)
            .unwrap_or_else(|| "--:--".into());
        format!("{} — {}  [{}]", self.artist, self.title, dur)
    }

    /// 形如 `Radiohead — Jigsaw Falling Into Place`，用于状态栏
    pub fn label(&self) -> String {
        format!("{} — {}", self.artist, self.title)
    }
}

/// 递归扫描目录，返回按路径排序的曲目列表。
pub fn scan_audio(dir: &Path) -> std::io::Result<Vec<Track>> {
    let mut tracks = Vec::new();
    collect_into(dir, &mut tracks)?;
    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(tracks)
}

/// 把 `dir` 下的音频文件收进 `out`，然后递归进每个子目录。
///
/// 单个子目录读不了（权限、被删等）只提示并跳过，不中断整体扫描。
fn collect_into(dir: &Path, out: &mut Vec<Track>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        // file_type() 不跟随符号链接：符号链接既不是 dir 也不是 file，
        // 所以天然避免了循环链接造成的无限递归
        let Ok(kind) = entry.file_type() else {
            continue;
        };

        if kind.is_dir() {
            if let Err(e) = collect_into(&path, out) {
                eprintln!("skip {}: {e}", path.display());
            }
        } else if kind.is_file() && is_audio(&path) {
            out.push(Track::from_path(path));
        }
    }
    Ok(())
}

/// 按扩展名判断是不是音频文件。
fn is_audio(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref(),
        Some("mp3" | "wav" | "ogg" | "m4a" | "aac" | "flac")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_audio_extensions() {
        assert!(is_audio(Path::new("song.mp3")));
        assert!(is_audio(Path::new("song.MP3"))); // 大小写不敏感
        assert!(!is_audio(Path::new("cover.jpg")));
        assert!(!is_audio(Path::new("noext")));
    }

    #[test]
    fn scans_subdirectories_recursively() {
        let root = std::env::temp_dir().join("musicplayer_scan_test");
        let _ = fs::remove_dir_all(&root); // 清掉上次的残留
        fs::create_dir_all(root.join("album/nested")).unwrap();
        fs::write(root.join("a.mp3"), b"").unwrap();
        fs::write(root.join("album/b.flac"), b"").unwrap();
        fs::write(root.join("album/nested/c.ogg"), b"").unwrap();
        fs::write(root.join("album/cover.jpg"), b"").unwrap();

        let tracks = scan_audio(&root).unwrap();
        let names: Vec<String> = tracks
            .iter()
            .map(|t| t.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        assert_eq!(names, vec!["a.mp3", "b.flac", "c.ogg"]);

        fs::remove_dir_all(&root).ok();
    }
}
