use std::{
    fs::{read_dir, File},
    io::Write,
    thread::sleep,
    time::{Duration, Instant},
};

use crate::args::Url;
use crate::error::MainError;
use color_eyre::{eyre::Result, owo_colors::OwoColorize};
use fantoccini::{Client, Locator};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use reqwest::{Client as ReqClient, ClientBuilder as ReqClientBuilder};
use spinners::{Spinner, Spinners};
use std::process::Command;

struct ImageData {
    bytes: Vec<u8>,
    path: String,
}

pub async fn dl_mangareader(client: &Client, url: Url, dl_path: Option<&str>) -> Result<()> {
    let title = url
        .get_title_from_url()
        .unwrap_or_else(|| gen_rand().to_string());

    let dl_path = match dl_path {
        Some(p) => format!("{p}/{}", title),
        None => {
            format!("./download/{title}")
        }
    };

    std::fs::create_dir_all(&dl_path)
        .unwrap_or_else(|_| panic!("could not create dir_all @: {dl_path}"));

    let start = Instant::now();
    println!();
    let mut message = format!("{} -> {}", "navigating to", title.yellow(),);

    let mut sp = Spinner::new(Spinners::Arc, message);
    client.goto(&url.0).await?;

    select_reading_mode(client).await?;

    sp.stop_with_newline();

    sp = Spinner::new(Spinners::Arc, "counting panels".yellow().to_string());
    let max = extract_counter(client).await? - 1;
    sp.stop_with_newline();

    let mut img_data_vec: Vec<ImageData> = Vec::new();
    let req_client = ReqClientBuilder::new().timeout(Duration::from_millis(2500));
    let req_client = req_client.build()?;

    println!();
    for i in 0..max {
        download_panel_canvas(i, client, &dl_path, &mut img_data_vec, &req_client).await?;
        let message = format!(
            "{} {}",
            "downloading panel data".bright_green(),
            format_args!(
                "{} / {}",
                (i + 1).to_string().yellow().bold(),
                max.to_string().yellow().bold()
            )
        );
        sp = Spinner::new(Spinners::Dots3, message);
        client.execute("hozNextImage()", vec![]).await?;
    }
    sp.stop();

    let elapsed = start.elapsed();
    message = format!(
        "{} in {}",
        "download complete".bright_green(),
        format_args!("{:?}", elapsed.yellow().bold())
    );
    println!("\n{message}\n");

    sp = Spinner::new(
        Spinners::Triangle,
        "writing data to file".yellow().to_string(),
    );
    img_data_vec
        .into_par_iter()
        .for_each(|data| if let Err(_) = write_img(data) {});
    sp.stop_with_newline();

    sp = Spinner::new(Spinners::Triangle, "cropping imgs".yellow().to_string());
    crop_imgs(&dl_path)?;
    sp.stop_with_newline();

    Ok(())
}

async fn select_reading_mode(c: &Client) -> Result<()> {
    if let Ok(btn) = c
        .wait()
        .at_most(Duration::from_millis(2000))
        .for_element(Locator::Css("a.rtl-row:nth-child(2)"))
        .await
    {
        btn.click().await?;

        close_open_window(c).await?;

        // Second click
        btn.click().await?;
    }

    Ok(())
}

async fn download_img(
    index: u16,
    c: &Client,
    dl_path: &str,
    img_data_vec: &mut Vec<ImageData>,
    client: &ReqClient,
) -> Result<(), MainError> {
    // Try to locate the image element using the CSS selector
    if let Ok(e) = c
        .wait()
        .at_most(Duration::from_millis(1000))
        .for_element(Locator::Css(
            "div.ds-item.active > div.ds-image.loaded > img.image-horizontal",
        ))
        .await
    {
        // Retrieve the image URL from the 'src' attribute
        if let Some(img_url) = e.attr("src").await? {
            // Ensure the URL is valid before attempting to download
            let img_url = img_url.trim();
            if !img_url.is_empty() {
                // Download the image using reqwest
                let res = client.get(img_url);
                let res = res.send().await?;
                if res.status().is_success() {
                    let bytes = res.bytes().await?.to_vec();
                    let path = format!("{}/{}.jpg", dl_path, index);
                    let img = ImageData { bytes, path };
                    img_data_vec.push(img);
                }
            }
        }
    }
    Ok(())
}
async fn close_open_window(c: &Client) -> Result<(), MainError> {
    // Handle the ad by closing the newly opened tab
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

async fn _handle_popup(c: &Client) -> Result<(), MainError> {
    if let Ok(e) = c
        .find(Locator::Css("div[style*='z-index: 2147483647']"))
        .await
    {
        e.click().await?;

        c.execute(
            r#"
            var ad = document.querySelector("div[style*='z-index: 2147483647']");
            ad.style.display = 'none';  // or 'visibility: hidden'; to just hide without affecting layout
            "#,
            vec![],
        ).await?;
    }

    Ok(())
}

async fn extract_counter(c: &Client) -> Result<u16, MainError> {
    let pgs_elm = c
        .wait()
        .for_element(Locator::Css("span.hoz-total-image"))
        .await?;

    let text = pgs_elm.html(true).await?;

    text.parse::<u16>()
        .map_err(|_| MainError::ParseCounterElement(text))
}

async fn download_panel_canvas(
    index: u16,
    c: &Client,
    dl_path: &str,
    img_data_vec: &mut Vec<ImageData>,
    req_client: &ReqClient,
) -> Result<(), MainError> {
    let index = index + 1;

    if let Ok(canvas) = c
        .wait()
        .at_most(Duration::from_millis(1000))
        .for_element(Locator::Css(
            format!(
                "div.ds-item.active:nth-child({index}) > div:nth-child(1) > canvas:nth-child(2)"
            )
            .as_str(),
        ))
        .await
    {
        // Wait until canvas content is rendered (check if the canvas is not just black)
        let mut canvas_ready = false;
        let timeout = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::time::Instant::now() < timeout {
            let data_url = c.execute(
                r#"
                var canvas = document.querySelector('div.ds-item.active > div:nth-child(1) > canvas:nth-child(2)');
                return canvas ? canvas.toDataURL() : null;
                "#,
                vec![],
            ).await?;
            // Check if the image is not black
            if let Some(data_url) = data_url.as_str() {
                if !data_url.contains("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAA") {
                    // base64 for black image
                    sleep(Duration::from_millis(350));
                    canvas_ready = true;
                    break;
                }
            }

            // Check again in 200 ms
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        if canvas_ready {
            let ss = canvas.screenshot().await?;
            let path = format!("{dl_path}/{index}.jpg");
            let img = ImageData { bytes: ss, path };
            img_data_vec.push(img);
        } else {
            println!("Canvas did not load in time.");
        }
    } else {
        download_img(index, c, dl_path, img_data_vec, req_client).await?;
    }

    Ok(())
}

fn write_img(data: ImageData) -> Result<()> {
    let mut file = File::create(&data.path)?;
    file.write_all(&data.bytes)?;
    Ok(())
}

pub fn crop_imgs(dl_path: &str) -> Result<(), MainError> {
    for (i, entry) in read_dir(dl_path)?.enumerate() {
        let entry = entry?;
        let path = entry.path();

        let path = path.to_string_lossy();

        Command::new("magick")
            .arg(path.as_ref()) // Path to the image
            .arg("-fuzz")
            .arg("10%")
            .arg("-fill")
            .arg("transparent")
            .arg("-opaque")
            .arg("rgb(17,17,17)")
            .arg("-trim")
            .arg("+repage")
            .arg(path.as_ref()) // Output file name
            .spawn()
            .expect("error launching magick");

        print!("\r{i}");
    }

    Ok(())
}

fn gen_rand() -> i32 {
    let num = vec![2, 3, 50, 80, 23124];
    let add = &num as *const Vec<i32>;
    add as i32
}
