use rodio::{Decoder,DeviceSinkBuilder,Player,Source};
use std::fs::File;
use std::env;
use std::time::Duration;
use std::thread;
use std::io::Write;

fn fmt_time(d: Duration) -> String {
    format!("{:02}:{:02}", d.as_secs() / 60, d.as_secs() % 60)
}

fn main() -> Result<(),Box<dyn std::error::Error>>{
    let path= match env::args().nth(1) {
        Some(p)=>p,
        None=>{eprintln!("use musicplayer <path>"); std::process::exit(1);},
    };

    let mut sink=DeviceSinkBuilder::open_default_sink()?;
    sink.log_on_drop(false);
    let player=Player::connect_new(sink.mixer());

    let file=File::open(&path).map_err(|e| format!("{} , {e}",path))?;
    let source=Decoder::try_from(file)?;
    let total_duration=source.total_duration();

    player.append(source);

    while !player.empty() {
        let pos=player.get_pos();
        match total_duration {
            Some(d) => println!("{} / {}", fmt_time(pos), fmt_time(d)),
            None => println!("{}", fmt_time(pos)),
        }
        std::io::stdout().flush().ok();
        thread::sleep(Duration::from_millis(200));
    }
    println!("");
    Ok(())
}
