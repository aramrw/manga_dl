use std::str::FromStr;

use clap::Parser;
use color_eyre::owo_colors::OwoColorize;

use crate::error::ArgError;

/// manga_dl Url argument
#[derive(Debug, Clone)]
pub struct Url(pub String);

/// Arguments passed to the manga_dl cli
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, num_args = 1..)]
    pub urls: Vec<Url>,
    /// Use an absolute path instead of manga_dl exe dir
    #[arg(short, long)]
    pub dl_path: Option<String>,
}

impl Args {
    pub fn check_urls(&mut self) -> Result<(), ArgError> {
        for url in self.urls.iter_mut() {
            url.check_url()?;
        }
        Ok(())
    }
}

pub fn get_args() -> Result<Args, ArgError> {
    let mut args = Args::parse();
    args.check_urls()?;

    Ok(args)
}

// Implement FromStr for Url so that clap can parse it as an argument
impl FromStr for Url {
    type Err = clap::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut url = Url(s.to_string());
        url.check_url()
            .map_err(|e| clap::Error::raw(clap::error::ErrorKind::InvalidValue, e))?;
        Ok(url)
    }
}

impl Url {
    pub fn get_title_from_url(&self) -> Option<String> {
        if self.0.contains("mangareader") {
            if let Some(start) = self.0.split_once("/read/") {
                // Extract the part after "/read/" until the next "/"
                return Some(start.1.replace("/", "-").to_string());
            }
        }
        None
    }

    pub fn check_url(&mut self) -> Result<(), ArgError> {
        if !self.0.contains("https://") {
            let new_url = format!("https://{}", self.0);
            self.0 = new_url;
        }

        let url = self.0.to_lowercase();

        if !url.is_ascii() {
            return Err(ArgError::InvalidUrl {
                url,
                reason: "URL is not valid ASCII".bold().to_string(),
                example: "*.com | *.to".yellow().to_string(),
            });
        }

        if url.is_empty() {
            return Err(ArgError::InvalidUrl {
                url,
                reason: "Empty string".bold().to_string(),
                example: "*.com | *.to".yellow().to_string(),
            });
        }

        if !(url.contains(".to") || url.contains(".com")) {
            return Err(ArgError::InvalidUrl {
                url,
                reason: "URL is missing a valid top-level domain".bold().to_string(),
                example: "*.com | *.to".yellow().to_string(),
            });
        }

        if !url.contains("mangareader.to") {
            return Err(ArgError::WebsiteNotSupported(url));
        }

        if url.contains("mangareader.to") && !url.contains("/read") {
            return Err(ArgError::InvalidUrl {
                url,
                reason: "mangareader URL is missing /read".bold().to_string(),
                example: "mangareader.to/read/vagabond-4/ja/chapter-6"
                    .yellow()
                    .to_string(),
            });
        }

        Ok(())
    }
}
