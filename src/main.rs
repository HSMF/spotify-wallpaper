use std::{
    ffi::OsStr,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::Stdio,
};

use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
struct App {
    #[clap(long, short)]
    wallpapers_dir: Option<String>,
    #[clap(long, short)]
    default: Option<String>,
}

#[derive(Deserialize, Serialize, Debug)]
struct Song {
    artist: String,
    title: String,
    status: String,
}

struct MyApp {
    wallpapers_dir: String,
    default: Option<String>,
}

fn home_dir() -> Option<String> {
    dirs::home_dir().and_then(|x| x.to_str().map(|x| x.to_owned()))
}

fn envs(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

impl From<App> for MyApp {
    fn from(value: App) -> Self {
        let wallpapers_dir = value
            .wallpapers_dir
            .unwrap_or_else(|| String::from("~/Pictures/wallpapers/"));
        let wallpapers_dir =
            shellexpand::full_with_context_no_errors(&wallpapers_dir, home_dir, envs);
        let wallpapers_dir = String::from(wallpapers_dir);
        let default = value
            .default
            .map(|x| shellexpand::full_with_context_no_errors(&x, home_dir, envs).to_string());
        MyApp {
            wallpapers_dir,
            default,
        }
    }
}

fn is_image(ext: Option<&OsStr>) -> bool {
    let ext = ext.and_then(|x| x.to_str());

    matches!(ext, Some("png" | "jpg" | "jpeg" | "gif"))
}

impl MyApp {
    fn process_song(&self, s: &str) -> anyhow::Result<()> {
        let song: Song = serde_xml_rs::from_str(s).context("failed to parse xml output")?;
        let wallpaper = self.get_wallpaper(&song)?;

        match (wallpaper, &self.default) {
            (None, None) => (),
            (None, Some(default)) => self.set_wallpaper(default)?,
            (Some(wallpaper), _) => self.set_wallpaper(wallpaper)?,
        }
        Ok(())
    }

    fn get_wallpaper(&self, song: &Song) -> anyhow::Result<Option<PathBuf>> {
        for entry in WalkDir::new(&self.wallpapers_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|x| x.file_type().is_file())
            .filter(|x| is_image(x.path().extension()))
        {
            let path = entry.path();
            let name = path.file_stem();
            if name.is_some_and(|x| x.eq_ignore_ascii_case(&*song.artist)) {
                return Ok(Some(path.to_owned()));
            }
        }
        Ok(None)
    }

    fn set_wallpaper<T: AsRef<OsStr>>(&self, wallpaper: T) -> anyhow::Result<()> {
        let mut child = std::process::Command::new("feh")
            .arg("--bg-fill")
            .arg(wallpaper.as_ref())
            .spawn()?;
        child.wait()?;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let app = App::parse();
    let mut child = std::process::Command::new("playerctl").args([
        "metadata",
        "--format",
        "<Song><artist>{{markup_escape(artist)}}</artist><title>{{title}}</title><status>{{status}}</status></Song>",
        "--follow"
    ]).stdout(Stdio::piped()).spawn()?;
    let stdout = child.stdout.take().context("Failed to open stdout")?;

    let app: MyApp = app.into();

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = line.context("failed to read line from output")?;
            app.process_song(&line)?;
        }

        Ok::<_, anyhow::Error>(())
    });

    child.wait()?;

    Ok(())
}
