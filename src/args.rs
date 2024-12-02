use clap::Parser;
use color_eyre::owo_colors::OwoColorize;

use crate::error::ArgError;

#[derive(Debug, Clone, Default)]
pub enum SupportedSites {
    #[default]
    MangaReader,
    MangaGun,
}

/// manga_dl Url argument
#[derive(Debug, Clone)]
pub struct Url {
    pub url: String,
    pub title: Option<String>,
    pub site: SupportedSites,
}

/// Arguments passed to the manga_dl cli
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long, num_args = 1..)]
    pub urls: Vec<String>,
    /// Specify specific panel indexes to download from a url (ie. if they didn't download properly).
    #[arg(long, num_args = 1..)]
    pub indexes: Option<Vec<usize>>,
    /// Use an absolute path to download images to. Defaults to ./download if not specified.
    #[arg(short, long)]
    pub dl_path: Option<String>,
    #[arg(long)]
    pub debug: bool,
}

impl Args {
    pub fn check_urls(&self) -> Result<Vec<Url>, ArgError> {
        let mut urls = Vec::new();
        for url in &self.urls {
            let url = Url::from_str(url)?;
            urls.push(url);
        }
        Ok(urls)
    }
}

pub fn get_args() -> Result<Args, ArgError> {
    let args = Args::parse();
    args.check_urls()?;

    Ok(args)
}

impl Url {
    fn from_str(s: impl AsRef<str>) -> Result<Self, ArgError> {
        let s = s.as_ref();
        let valid_url_str = Url::check_url(s.to_string())?;
        let site = Url::verify_supported_site(&valid_url_str)?;
        let title = Url::get_title_from_valid_url(&valid_url_str, &site);
        let url = Url {
            url: valid_url_str,
            title,
            site,
        };

        Ok(url)
    }

    fn verify_supported_site(url: &str) -> Result<SupportedSites, ArgError> {
        if url.contains("mangareader") {
            return Ok(SupportedSites::MangaReader);
        } else if url.contains("mangagun") {
            return Ok(SupportedSites::MangaGun);
        }
        Err(ArgError::WebsiteNotSupported(url.to_string()))
    }

    pub fn get_title_from_valid_url(url: &str, site: &SupportedSites) -> Option<String> {
        match site {
            SupportedSites::MangaReader => {
                if let Some(start) = url.split_once("/read/") {
                    // Extract the part after "/read/" until the next "/"
                    return Some(start.1.replace("/", "-").to_string());
                }
            }
            SupportedSites::MangaGun => {
                if let Some(start) = url.rsplit_once("/") {
                    return Some(start.1.to_string());
                }
            }
        }
        None
    }

    pub fn check_url(url: String) -> Result<String, ArgError> {
        let mut url = url;
        if !url.contains("https://") {
            let new_url = format!("https://{}", url);
            url = new_url;
        }

        let url = url.to_lowercase();

        if !url.is_ascii() {
            return Err(ArgError::InvalidUrl {
                url,
                reason: "URL is not valid ASCII".bold().to_string(),
                example: "*.com | *.to | *.net".yellow().to_string(),
            });
        }

        if url.is_empty() {
            return Err(ArgError::InvalidUrl {
                url,
                reason: "Empty string".bold().to_string(),
                example: "*.com | *.to".yellow().to_string(),
            });
        }

        if !(url.contains(".to") || url.contains(".com") || url.contains(".net")) {
            return Err(ArgError::InvalidUrl {
                url,
                reason: "URL is missing a valid top-level domain".bold().to_string(),
                example: "*.com | *.to".yellow().to_string(),
            });
        }

        if !(url.contains("mangareader.to") || url.contains("mangagun.net")) {
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

        Ok(url)
    }
}
