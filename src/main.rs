mod args;
mod error;
mod mangareader;
mod test;

use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
    process::{Child, Command},
};

use crate::mangareader::dl_mangareader;
use args::get_args;
use color_eyre::{eyre::Result, owo_colors::OwoColorize};
use error::MainError;
use fantoccini::{wd::Capabilities, Client};
use serde_json::{json, to_string_pretty};
use spinners::{Spinner, Spinners};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().unwrap();

    #[cfg(target_os = "windows")]
    let gd_data: &[u8] = include_bytes!("../bin/geckodriver-win.exe");
    #[cfg(target_os = "macos")]
    let gd_data: &[u8] = include_bytes!("../bin/geckodriver-macos");

    let args = get_args()?;

    let child = start_gd(gd_data).expect("failed to start gecko driver");
    let c: Client = start_client().await.expect("failed to start fantoccini");
    let mut errs = Vec::new();

    for url in args.urls {
        match dl_mangareader(&c, url, args.dl_path.as_deref()).await {
            Ok(_) => {}
            Err(e) => errs.push(e),
        };
    }

    c.close().await?;
    cleanup(child);

    if !errs.is_empty() {
        let msg = format!("{} {}", errs.len().bold().red(), "errors occured".red());
        println!("{msg}");
        for (i, e) in errs.into_iter().enumerate() {
            write_log(i, MainError::ColorEyre(e))?;
        }
    }

    Ok(())
}

pub fn write_log(i: usize, e: MainError) -> Result<(), io::Error> {
    let mut f = File::create("manga_dl_errors.log")?;
    let err = json!({
        "index": i,
        "error": e.to_string(),
    });
    let err = to_string_pretty(&err)?;
    f.write_all(err.as_bytes())?;
    f.flush()?;

    Ok(())
}

pub fn cleanup(mut child: Child) {
    let message = format!(
        "{} {}",
        "performing cleanup,".yellow(),
        "do not exit the program!".red().bold()
    );
    let mut sp = Spinner::new(Spinners::Triangle, message);

    child.kill().expect("Failed to kill geckodriver: {}");

    child
        .wait()
        .expect("error while waiting for geckodriver: {}");

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

    let child = Command::new(temp_path).spawn()?;

    Ok(child)
}

async fn start_client() -> Result<fantoccini::Client, fantoccini::error::NewSessionError> {
    let caps: Capabilities = json!({
        "moz:firefoxOptions": {
            "args": ["-headless"]
        }
    })
    .as_object()
    .expect("failed to serialize caps")
    .clone();

    fantoccini::ClientBuilder::native()
        .capabilities(caps)
        .connect("http://localhost:4444")
        .await
}
