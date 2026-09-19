//! 数据层：一首歌的数据结构，以及把文件系统变成歌单。
//!
//! 这一层完全不碰终端、也不碰音频设备，可以单独测试。
//!
//! 关于"懒加载"：`Track` 刚建出来时只有路径，标签是 `None`。
//! 扫描只负责收集路径（快），标签由后台线程用 `read_meta` 逐个补上（慢）。

use lofty::prelude::*;
use lofty::read_from_path;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::util::fmt_time;

/// 从文件里读出来的信息。
///
/// 注意 `meta: Some(..)` 表示"读过了"，不代表"读到内容丰富"——
/// 一个完全没标签的文件也会得到 `Some`，只是里面是兜底值。
/// 这样才能把"还没读"和"读了但没标签"区分开。
pub struct TrackMeta {
    title: String,
    artist: String,
    album: String,
    duration: Option<Duration>,
}

/// 一首歌 = 一个路径 + 可能已经读到的标签。
pub struct Track {
    path: PathBuf,
    meta: Option<TrackMeta>,
}

impl Track {
    /// 只记录路径，不读标签 —— 扫描时用这个，所以很快。
    pub fn new(path: PathBuf) -> Self {
        Track { path, meta: None }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 标签是否已经读过（UI 用它决定要不要显示成灰色）
    pub fn is_loaded(&self) -> bool {
        self.meta.is_some()
    }

    pub fn set_meta(&mut self, meta: TrackMeta) {
        self.meta = Some(meta);
    }

    /// 如果还没读过标签，就现在同步读一次（约 1ms）。
    ///
    /// 用在"光标移到这一首"或"要播放这一首"的时候，
    /// 这样用户不用等后台线程慢慢排到它。
    pub fn ensure_meta(&mut self) {
        if self.meta.is_none() {
            self.meta = Some(read_meta(&self.path));
        }
    }

    /// 没有标签时的兜底名字 = 文件名
    fn fallback_name(&self) -> String {
        self.path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    /// 列表里那一行
    pub fn display_line(&self) -> String {
        match &self.meta {
            Some(m) => {
                let dur = m.duration.map(fmt_time).unwrap_or_else(|| "--:--".into());
                format!("{} — {}  [{}]", m.artist, m.title, dur)
            }
            None => self.fallback_name(),
        }
    }

    /// 状态栏用的名字
    pub fn label(&self) -> String {
        match &self.meta {
            Some(m) => format!("{} — {}", m.artist, m.title),
            None => self.fallback_name(),
        }
    }
}

/// 读一个音频文件的标签和音频属性。
///
/// **永远返回 `TrackMeta`**：读不到标签就用兜底值，
/// 这样调用方不需要区分"失败"和"空标签"两种情况。
pub fn read_meta(path: &Path) -> TrackMeta {
    let mut meta = TrackMeta {
        title: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        artist: "Unknown Artist".to_string(),
        album: "Unknown Album".to_string(),
        duration: None,
    };

    // 解析失败（文件损坏、不是音频等）就用兜底值，不报错
    if let Ok(tagged) = read_from_path(path) {
        // duration 来自音频流属性，不是标签
        meta.duration = Some(tagged.properties().duration());

        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(title) = tag.title() {
                meta.title = title.into_owned();
            }
            if let Some(artist) = tag.artist() {
                meta.artist = artist.into_owned();
            }
            if let Some(album) = tag.album() {
                meta.album = album.into_owned();
            }
        }
    }

    meta
}

/// 递归扫描目录，返回按路径排序的曲目列表。
///
/// 只收集路径，不读标签，所以几千首歌也是瞬间完成。
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
            out.push(Track::new(path));
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

    #[test]
    fn scan_does_not_read_tags() {
        // 假的 .mp3 内容不可能解析出标签，但扫描依然成功且不算"已加载"
        let root = std::env::temp_dir().join("musicplayer_lazy_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("fake.mp3"), b"not really an mp3").unwrap();

        let tracks = scan_audio(&root).unwrap();
        assert_eq!(tracks.len(), 1);
        assert!(!tracks[0].is_loaded()); // 扫描阶段不该读标签

        // 兜底名字来自文件名
        assert_eq!(tracks[0].display_line(), "fake");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn read_meta_falls_back_for_broken_files() {
        let path = std::env::temp_dir().join("musicplayer_broken_test.mp3");
        fs::write(&path, b"definitely not audio").unwrap();

        let meta = read_meta(&path);

        assert_eq!(meta.title, "musicplayer_broken_test");
        assert_eq!(meta.artist, "Unknown Artist");
        assert!(meta.duration.is_none());

        fs::remove_file(&path).ok();
    }
}
