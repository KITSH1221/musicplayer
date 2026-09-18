use lofty::prelude::*;
use lofty::read_from_path;
use rodio::{Decoder, DeviceSinkBuilder, Player, Source};
use std::{
    env,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

fn fmt_time(d: Duration) -> String {
    format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60)
}

struct Track {
    path: PathBuf,
    title: String,
    artist: String,
    album: String,
    duration: Option<Duration>,
}

impl Track {
    fn from_path(path: PathBuf) -> Self {
        // ① 先用文件名做兜底标题——注意必须在 path 被移进结构体之前算
        let fallback = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let mut track = Track {
            path,
            title: fallback,
            artist: "unknown".to_string(),
            album: "unknown".to_string(),
            duration: None,
        };
        if let Ok(tagged) = read_from_path(&track.path) {
            track.duration = Some(tagged.properties().duration());
            // ② 从标签中提取标题、艺术家、专辑
            if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
                if let Some(t) = tag.title() {
                    track.title = t.into_owned();
                }
                if let Some(a) = tag.artist() {
                    track.artist = a.into_owned();
                }
                if let Some(a) = tag.album() {
                    track.album = a.into_owned();
                }
            }
        }
        track
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = match env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("use musicplayer <path>");
            std::process::exit(1);
        }
    };
    let target = PathBuf::from(path);
    let playlist = if target.is_dir() {
        scan_audio(&target)?
    } else {
        vec![Track::from_path(target)]
    };
    if playlist.is_empty() {
        eprintln!("no audio files found");
        std::process::exit(1);
    }

    println!("playlist {}", playlist.len());
    for (i, t) in playlist.iter().enumerate() {
        let dur = t.duration.map(fmt_time).unwrap_or_else(|| "--:--".into());
        println!("  {:2}. {} — {}  [{}]", i + 1, t.artist, t.title, dur);
    }

    let mut sink = DeviceSinkBuilder::open_default_sink()?;
    sink.log_on_drop(false);
    let player = Player::connect_new(sink.mixer());

    for t in playlist {
        let file = File::open(&t.path).map_err(|e| format!("{} , {e}", t.path.display()))?;
        let source = Decoder::try_from(file)?;
        let total_duration = source.total_duration();
        println!("\n▶ {} — {}", t.artist, t.title);

        player.append(source);

        while !player.empty() {
            let pos = player.get_pos();
            match total_duration {
                Some(d) => println!("{} / {}", fmt_time(pos), fmt_time(d)),
                None => println!("{}", fmt_time(pos)),
            }
            std::io::stdout().flush().ok();
            thread::sleep(Duration::from_millis(200));
        }
        println!("");
    }
    Ok(())
}

fn is_audio(path: &Path) -> bool {
    // match macro => matches!(expression,pattern)
    matches! {
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
        Some("mp3" | "wav" | "ogg" | "m4a" | "aac" | "flac")
    }
}
fn scan_audio(dir: &Path) -> std::io::Result<Vec<Track>> {
    let mut tracks = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_audio(&path) {
            tracks.push(Track::from_path(path));
        }
    }
    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(tracks)
}
