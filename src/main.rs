#![feature(async_closure)]

mod cli;
mod error;
mod loading;
mod macros;
mod mangagun;
mod mangareader;
mod rawmanga;

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
    process::{Child, Command},
    time,
};

use crate::mangagun::dl_mangagun;
use crate::mangareader::dl_mangareader;
use cli::{get_args, Cli, LogLevel, Url};
#[allow(unused_imports)]
use color_eyre::{eyre::Result, owo_colors::OwoColorize, Report};
use error::{MainError, MangaReaderError};
use fantoccini::{Client, Locator};
use loading::{print_indexes_arg, print_reqerr_count};
use mangagun::NavigateGroup;
use rawmanga::dl_rawmanga;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_string_pretty};
use spinners::Spinner;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install().unwrap();
    #[cfg(target_os = "windows")]
    let instant = time::Instant::now();
    let args = get_args()?;
    println!("{:#?}", args.log);
    match args.log {
        LogLevel::Full | LogLevel::Verbose => {
            std::env::set_var("RUST_BACKTRACE", "1");
        }
        _ => { /* skip */ }
    }

    #[cfg(target_os = "windows")]
    let gd_data: &[u8] = include_bytes!("../bin/geckodriver-win.exe");
    #[cfg(target_os = "macos")]
    let gd_data: &[u8] = include_bytes!("../bin/geckodriver-macos");

    #[allow(clippy::zombie_processes)]
    let mut child = start_gd(gd_data).expect("failed to start gecko driver");
    let c: Client = start_client(&args.log)
        .await
        .expect("failed to start fantoccini");
    let mut errors: Vec<Report> = Vec::new();
    let urls = args.check_urls()?;

    if let Some(indexes) = &args.indexes {
        print_indexes_arg(indexes);
    }

    for (i, url) in urls.iter().enumerate() {
        match url.site {
            cli::SupportedSites::MangaReader => {
                if let Err(e) = dl_mangareader(&c, url, &args, i).await {
                    errors.push(e);
                };
            }
            cli::SupportedSites::MangaGun => {
                if let Err(e) = dl_mangagun(&c, url, &args).await {
                    errors.push(e);
                };
            }
            cli::SupportedSites::RawManga => {
                if let Err(e) = dl_rawmanga(&c, url, &args).await {
                    errors.push(e);
                };
            }
        }
    }

    c.close().await?;
    child.kill().expect("failed to kill geckodriver");
    child
        .wait()
        .expect("panicked while waiting for geckodriver to exit after attempting terminate");
    cleanup();

    if !errors.is_empty() {
        let titles: Vec<String> = urls.into_iter().flat_map(|url| url.title).collect();
        print_reqerr_count(errors.len(), &titles);
        eprintln!("{}", style_text!("STDERR:", error));
        for e in errors {
            eprintln!("{:?}", e);
        }
    }

    let elap = instant.elapsed().as_secs();
    println!("\nelapsed: {}s", elap);

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
    #[cfg(target_os = "windows")]
    fs::remove_file("./temp/gd.exe").expect("failed to remove gd.exe");
    #[cfg(target_os = "macos")]
    fs::remove_file("./temp/gd.exe").expect("failed to remove gd executable");

    fs::remove_dir("temp").expect("failed to remove temp dir");
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

    // Set execute permission (for UNIX systems)
    #[cfg(unix)]
    {
        let metadata = std::fs::metadata(&temp_path)?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755); // Make the file executable
        std::fs::set_permissions(&temp_path, permissions)?;
    }

    let child = Command::new(temp_path)
        .arg("--binary")
        .arg("C:\\Program Files\\Mozilla Firefox\\firefox.exe")
        .spawn()?;

    Ok(child)
}

async fn start_client(log: &LogLevel) -> Result<Client, fantoccini::error::NewSessionError> {
    let mut builder = fantoccini::ClientBuilder::native();

    if log != &LogLevel::Full {
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

    builder.connect("http://localhost:4444").await
}

/// handle any redirect ads by closing the newly opened tab
pub async fn g_close_open_window(c: &Client) -> Result<(), MangaReaderError> {
    let handles = c.windows().await?;
    if handles.len() > 1 {
        for handle in handles.iter().skip(1) {
            c.switch_to_window(handle.clone()).await?;
            c.close_window().await?;
        }
        c.switch_to_window(handles[0].clone()).await?;
    }

    Ok(())
}

/// hides anything with a z-index of `2147483647`.
pub async fn g_handle_popup(c: &Client) -> Result<(), MainError> {
    if let Ok(e) = c
        .find(Locator::Css("*[style*='z-index: 2147483647']"))
        .await
    {
        e.click().await?;

        c.execute(
            r#"
            var ad = document.querySelector("*[style*='z-index: 2147483647']");
            ad.style.display = 'none';  
            "#,
            vec![],
        )
        .await?;
    }

    Ok(())
}

pub async fn setup_nav(client: &Client, url: &Url, args: &Cli) -> Result<NavigateGroup> {
    let title = url.title.clone().unwrap_or_else(|| gen_rand().to_string());
    let dl_path = match &args.input_path {
        Some(p) => format!("{p}/{}", title),
        None => {
            format!("./download/{title}")
        }
    };

    if let Err(e) = std::fs::create_dir_all(&dl_path) {
        panic!("{e} \n            at: `{dl_path}`");
    }

    println!("\n{:?}", url.site);
    let message = format!("{}: {}", "", style_text!(&title, url));
    let mut sp = Spinner::new(spinners::Spinners::Arc, message);
    client.goto(&url.url).await?;
    sp.stop_with_newline();

    Ok((title, dl_path, sp))
}

pub fn gen_rand() -> i32 {
    let num = vec![2, 3, 50, 80, 23124];
    let add = &num as *const Vec<i32>;
    add as i32
}
