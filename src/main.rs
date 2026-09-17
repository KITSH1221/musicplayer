use rodio::{Decoder, DeviceSinkBuilder, Player, Source};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::{env, path};

fn fmt_time(d: Duration) -> String {
    format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60)
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
        vec![target]
    };
    if playlist.is_empty() {
        eprintln!("no audio files found");
        std::process::exit(1);
    }

    println!("playlist {}", playlist.len());
    for (i, p) in playlist.iter().enumerate() {
        println!(
            "{:2}. {}",
            i + 1,
            p.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    let mut sink = DeviceSinkBuilder::open_default_sink()?;
    sink.log_on_drop(false);
    let player = Player::connect_new(sink.mixer());

    for path in playlist {
        let file = File::open(&path).map_err(|e| format!("{} , {e}", path.display()))?;
        let source = Decoder::try_from(file)?;
        let total_duration = source.total_duration();

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
fn scan_audio(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_audio(&path) {
            files.push(path);
        }
    }
    Ok(files)
}
