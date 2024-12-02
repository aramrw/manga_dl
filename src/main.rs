mod args;
mod error;
mod loading;
mod mangagun;
mod mangareader;
mod test;

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
    process::{Child, Command},
};

use crate::mangagun::dl_mangagun;
use crate::mangareader::dl_mangareader;
use args::get_args;
use color_eyre::{eyre::Result, owo_colors::OwoColorize};
use fantoccini::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_string_pretty};
use spinners::{Spinner, Spinners};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().unwrap();
    let args = get_args()?;

    #[cfg(target_os = "windows")]
    let gd_data: &[u8] = include_bytes!("../bin/geckodriver-win.exe");
    #[cfg(target_os = "macos")]
    let gd_data: &[u8] = include_bytes!("../bin/geckodriver-macos");

    let mut child = start_gd(gd_data).expect("failed to start gecko driver");
    let c: Client = start_client(args.debug)
        .await
        .expect("failed to start fantoccini");
    let mut errs: Vec<LogError> = Vec::new();

    let urls = args.check_urls()?;

    for (i, url) in urls.into_iter().enumerate() {
        let mut report: Option<color_eyre::Report> = None;
        match url.site {
            args::SupportedSites::MangaReader => {
                if let Err(err) = dl_mangareader(&c, &url, args.dl_path.as_deref(), i).await {
                    report = Some(err);
                };
            }
            args::SupportedSites::MangaGun => {
                if let Err(err) = dl_mangagun(&c, &url, &args).await {
                    report = Some(err);
                };
            }
        }
        if let Some(report) = report {
            let err = LogError {
                url: url.url,
                index: i,
                error: report.to_string(),
            };
            errs.push(err);
        }
    }

    c.close().await?;
    child.kill().expect("failed to kill geckodriver");
    child
        .wait()
        .expect("panicked while waiting for geckodriver to exit after attempting terminate");
    cleanup();

    if !errs.is_empty() {
        let msg = format!("{} {}", errs.len().bold().red(), "errors occured".red());
        println!("{msg}");
        for e in errs.into_iter() {
            write_log(e)?;
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LogError {
    url: String,
    index: usize,
    error: String,
}

pub fn write_log(e: LogError) -> Result<(), io::Error> {
    let mut f = OpenOptions::new()
        .append(true)
        .create(true)
        .open("manga_dl_errors.log")?;

    let err = to_string_pretty(&e)?;
    f.write_all(err.as_bytes())?;
    f.flush()?;

    Ok(())
}

pub fn cleanup() {
    let message = format!("{}..", "performing cleanup".yellow(),);
    let mut sp = Spinner::new(Spinners::Triangle, message);

    #[cfg(target_os = "windows")]
    fs::remove_file("./temp/gd.exe").expect("failed to remove gd.exe");
    #[cfg(target_os = "macos")]
    fs::remove_file("./temp/gd.exe").expect("failed to remove gd executable");

    fs::remove_dir("temp").expect("failed to remove temp dir");

    sp.stop_with_newline();
}

pub fn start_gd(gd_data: &[u8]) -> Result<Child, std::io::Error> {
    fs::create_dir_all("temp")?;
    #[cfg(target_os = "windows")]
    let temp_path = Path::new("temp/gd.exe");
    #[cfg(target_os = "macos")]
    let temp_path = Path::new("temp/gd");

    if !temp_path.exists() {
        let mut temp_file = File::create_new(temp_path)?;
        temp_file.write_all(gd_data)?;
        drop(temp_file);
    }

    // // Set execute permission (for UNIX systems)
    // #[cfg(unix)]
    // {
    //     let metadata = std::fs::metadata(&temp_path)?;
    //     let mut permissions = metadata.permissions();
    //     permissions.set_mode(0o755); // Make the file executable
    //     std::fs::set_permissions(&temp_path, permissions)?;
    // }

    let child = Command::new(temp_path)
        .arg("--binary")
        .arg("C:\\Program Files\\Mozilla Firefox\\firefox.exe")
        .spawn()?;

    Ok(child)
}

async fn start_client(debug: bool) -> Result<Client, fantoccini::error::NewSessionError> {
    // Default builder
    let mut builder = fantoccini::ClientBuilder::native();

    // Conditionally add capabilities
    if !debug {
        let caps: serde_json::Map<String, serde_json::Value> = json!({
            "moz:firefoxOptions": {
                "args": ["-headless"]
            }
        })
        .as_object()
        .expect("failed to serialize caps")
        .clone();

        builder.capabilities(caps);
    }

    // Connect to the WebDriver
    builder.connect("http://localhost:4444").await
}
