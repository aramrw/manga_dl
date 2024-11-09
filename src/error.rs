use std::num::ParseIntError;
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
}
