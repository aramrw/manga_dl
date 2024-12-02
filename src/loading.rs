use std::time::Duration;

use color_eyre::owo_colors::OwoColorize;

pub fn print_indexes_arg(indexes: &Vec<usize>) {
    println!("only downlading indexes: {:?}.", indexes);
}

pub fn downloading_panel_data_msg(i: u16, max: u16) -> String {
    format!(
        "{} {}",
        "downloading panel data..".bright_green(),
        format_args!(
            "{} / {}.",
            (i + 1).to_string().yellow().bold(),
            max.to_string().yellow().bold()
        )
    )
}

pub fn fetching_img_bytes() -> String {
    format!("{}", "fetching img bytes...".bright_green(),)
}

pub fn indexes_failed_msg(len: usize) -> String {
    format!(
        "{} {}",
        len.yellow().bold(),
        "panels failed; trying again..".red()
    )
}

pub fn print_download_complete_msg(elapsed: Duration) {
    let msg = format!(
        "{} in {}.",
        "download complete".bold().bright_green(),
        format_args!("{:?}", elapsed.yellow().bold())
    );
    println!("{msg}\n");
}

pub fn print_reqerr_count(count: u16, title: &str) {
    let msg = format!(
        "{}: {} {}: {}.",
        "WARNING".bold().red(),
        count.bold(),
        "errors occured while downloading".red(),
        title.yellow().bold()
    );
    println!("{msg}");
}
