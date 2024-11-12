use thiserror::Error;

#[derive(Error, Debug)]
pub enum MainError {
    #[error("{0}")]
    IoErr(#[from] std::io::Error),
    #[error("{0}")]
    Fantoccini(#[from] fantoccini::error::CmdError),
    #[error("{0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("{0} could not be parsed")]
    ParseCounterElement(String),
    #[error("{0}")]
    Args(#[from] ArgError),
    #[error("{0}")]
    ColorEyre(#[from] color_eyre::Report),
    #[error("{0}")]
    JoinError(#[from] tokio::task::JoinError),
}

#[derive(Error, Debug)]
pub enum ArgError {
    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error(
        "--url argument: {url} is not a valid url\n\
             Reason: {reason}\n\
             Example: {example}"
    )]
    InvalidUrl {
        url: String,
        reason: String,
        example: String,
    },

    #[error("--url argument: {0} is not a supported site\nrun with --help for a list of supported sites.")]
    WebsiteNotSupported(String),
}
