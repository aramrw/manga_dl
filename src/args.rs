use clap::Parser;

use crate::error::ArgError;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short, long)]
    pub url: String,
}

pub fn get_args() -> Result<Args, ArgError> {
    let args = Args::parse();
    let url = &args.url;
    check_url(url)?;

    Ok(args)
}

pub fn check_url(url: &str) -> Result<(), ArgError> {
    let url = url.to_lowercase();

    if !url.is_ascii() {
        return Err(ArgError::InvalidUrl(url, "url is not valid ascii".into()));
    }
    if url.is_empty() {
        return Err(ArgError::InvalidUrl(url, "empty string".into()));
    }
    if !url.contains(".to") || !url.contains(".com") {
        return Err(ArgError::InvalidUrl(
            url,
            "url is missing a valid top level domain\nExample: *.com | *.to".into(),
        ));
    }

    if !url.contains("mangareader.to") {
        return Err(ArgError::WebsiteNotSupported(url));
    }

    Ok(())
}
