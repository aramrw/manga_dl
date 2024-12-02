use base64::prelude::*;
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::{read_dir, File},
    io::Write,
    time::{Duration, Instant},
};

use crate::{
    args::Url,
    error::DownloadImageError,
    loading::{indexes_failed_msg, print_download_complete_msg, print_reqerr_count},
    write_log, LogError,
};
use crate::{error::MainError, loading::downloading_panel_data_msg};
use color_eyre::{
    eyre::{Context, Result},
    owo_colors::OwoColorize,
};
use fantoccini::{Client, Locator};
use reqwest::{Client as ReqClient, ClientBuilder as ReqClientBuilder};
use spinners::{Spinner, Spinners};
use std::process::Command;

#[derive(Eq, Hash, PartialEq)]
pub struct ImageData {
    pub bytes: Vec<u8>,
    pub path: String,
}

#[derive(Eq, Hash, PartialEq)]
pub struct ReqImageData {
    pub url: String,
    pub path: String,
}

/// Arguments
///
/// * `index` - mangareader has a popup if youve have no cookies,
///
/// however on consequent loads its
/// not necessary to deal with.
pub async fn dl_mangareader(
    client: &Client,
    url: &Url,
    dl_path: Option<&str>,
    index: usize,
) -> Result<()> {
    let title = url.title.clone()
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
    let message = format!("{} -> {}", "navigating to", title.bold().yellow());

    let mut sp = Spinner::new(Spinners::Arc, message);
    client.goto(&url.url).await?;

    if index == 0 {
        select_reading_mode(client).await?;
    }

    sp.stop_with_newline();

    sp = Spinner::new(Spinners::Arc, "counting panels".yellow().to_string());
    let max = extract_counter(client).await? - 1;
    sp.stop_with_newline();

    // hold all the bytes and formatted paths of the imgs
    // to write all at once at the very end
    let mut img_data_vec: Vec<ImageData> = Vec::new();

    let req_client = ReqClientBuilder::new().timeout(Duration::from_millis(2500));
    let req_client = req_client.build()?;

    // if the panel_canvas is an <img> elemement get req is required
    // hold src urls till the end via this vector to download concurrently
    // (url, dl_path)
    let mut src_urls: HashSet<ReqImageData> = HashSet::new();
    let is_imgs = _find_images(client).await;
    let mut indexes_failed = HashSet::new();

    println!();
    for i in 0..max {
        if !is_imgs {
            match download_panel_canvas(i, client, &dl_path, &mut img_data_vec, Duration::from_secs(5)).await {
                Ok(_) => (),
                Err(DownloadImageError::MissingCanvasElement(_)) => {
                    if download_panel_img(i, client, &dl_path, &mut src_urls)
                        .await
                        .is_err()
                    {
                        indexes_failed.insert(i);
                    }
                }
                Err(e) => return Err(e.into()),
            };
        } else if let Err(img_err) = download_panel_img(i, client, &dl_path, &mut src_urls).await {
            if let Err(canvas_err) =
                download_panel_canvas(i, client, &dl_path, &mut img_data_vec, Duration::from_secs(5)).await
            {
                let err = LogError {
                    url: dl_path.to_string(),
                    index: i as usize,
                    error: format!("{} + {}", img_err, canvas_err),
                };
                write_log(err)?;
            };
        }
        sp = Spinner::new(Spinners::Dots3, downloading_panel_data_msg(i, max));
        client.execute("hozNextImage()", vec![]).await?;
    }
    sp.stop_with_newline();
    let mut reqerr_count = 0;

    if !indexes_failed.is_empty() {
        sp = Spinner::new(Spinners::Dots3, indexes_failed_msg(indexes_failed.len()));
        client.goto(&url.url).await?;
        for index in 0..max {
            if indexes_failed.contains(&index) {
                match download_panel_canvas(index, client, &dl_path, &mut img_data_vec, Duration::from_secs(10)).await {
                    Ok(_) => (),
                    Err(DownloadImageError::MissingCanvasElement(e)) => {
                        let err = LogError {
                            url: dl_path.to_string(),
                            index: index as usize,
                            error: format!("failed again on the second attempt to download: {}", e),
                        };
                        write_log(err)?;
                        reqerr_count += 1;
                    }
                    Err(e) => return Err(e.into()),
                };
            }
            client.execute("hozNextImage()", vec![]).await?;
        }

        sp.stop_with_newline();
    }

    if !src_urls.is_empty() {
        for (i, img) in src_urls.into_iter().enumerate() {
            let res = download_img_src(&img.url, img.path, req_client.clone()).await;

            match res {
                Ok(r) => img_data_vec.push(r),
                Err(e) => {
                    reqerr_count += 1;
                    let log_err = LogError {
                        url: img.url,
                        index: i,
                        error: e.to_string(),
                    };
                    let _ = write_log(log_err);
                }
            }
        }
    }
    //sp.stop_with_newline();

    if reqerr_count > 0 {
        print_reqerr_count(reqerr_count, &title);
    }

    let elapsed = start.elapsed();
    print_download_complete_msg(elapsed);

    sp = Spinner::new(
        Spinners::Triangle,
        "writing data to file..".yellow().to_string(),
    );

    img_data_vec
        .into_iter()
        .for_each(|data| write_img(data).unwrap());
    sp.stop_with_newline();

    Ok(())
}

async fn _find_images(c: &Client) -> bool {
    if let Ok(elements) = c.find_all(Locator::Css("img.image-horizontal")).await {
        for el in elements {
            if el.attr("src").await.is_ok() {
                return true;
            }
        }
    }
    false
}

async fn download_panel_canvas(
    index: u16,
    c: &Client,
    dl_path: &str,
    img_data_vec: &mut Vec<ImageData>,
    dur: Duration,
) -> Result<(), DownloadImageError> {
    // Construct the selector with the provided index
    let selector = "div.ds-item.active .image-horizontal";

    // Check if the canvas element exists at the given selector
    if c.wait()
        .at_most(dur)
        .for_element(Locator::Css(selector))
        .await
        .is_ok()
    {
        // Execute JavaScript to get the data URL of the canvas element
        let script = r#"
            var canvas = document.querySelector(arguments[0]);
            return canvas ? canvas.toDataURL() : null;
        "#;

        // Execute the script with the selector as argument
        let data_url = c
            .execute(script, vec![selector.into()])
            .await
            .expect("failed to execute js toDataUrl() script")
            .as_str()
            .unwrap()
            .trim()
            .to_string();

        let base64_data = data_url
            .strip_prefix("data:image/jpeg;base64,")
            .or_else(|| data_url.strip_prefix("data:image/png;base64,"))
            .ok_or_else(|| DownloadImageError::InvalidDataUrl(data_url.clone()))?;

        // Decode the Base64 string into binary data
        let decoded_data = BASE64_STANDARD
            .decode(base64_data)
            .expect("failed to decode base64 string into binary.");

        let index = index + 1;
        let path = format!("{dl_path}/{index}.jpg");

        // Add the image data to the vector
        let img = ImageData {
            bytes: decoded_data,
            path,
        };
        img_data_vec.push(img);
    } else {
        return Err(DownloadImageError::MissingCanvasElement(
            dl_path.to_string(),
        ));
    }

    Ok(())
}

async fn download_panel_img(
    index: u16,
    c: &Client,
    dl_path: &str,
    src_urls: &mut HashSet<ReqImageData>,
) -> Result<(), DownloadImageError> {
    // Try to locate the image element using the CSS selector
    if let Ok(elm) = c
        .wait()
        .at_most(Duration::from_millis(2000))
        .for_element(Locator::Css(
            "div.ds-item.active > div.ds-image.loaded > img.image-horizontal",
        ))
        .await
    {
        // Retrieve the image URL from the 'src' attribute
        if let Some(img_url) = elm.attr("src").await? {
            // Ensure the URL is valid before attempting to download
            let img_url = img_url.trim();
            if !img_url.is_empty() {
                let index = index + 1;
                let path = format!("{dl_path}/{index}.jpg");
                let img = ReqImageData {
                    url: img_url.to_string(),
                    path,
                };
                src_urls.insert(img);
            }
        }
    } else {
        return Err(DownloadImageError::MissingImgElement(dl_path.to_string()));
    }

    Ok(())
}

pub async fn download_img_src(
    url: &str,
    path: String,
    c: ReqClient,
) -> Result<ImageData, DownloadImageError> {
    let res = c
        .get(url)
        .send()
        .await
        .map_err(|e| DownloadImageError::GetReqwest(url.to_string(), e.to_string()))?;
    let bytes = res
        .bytes()
        .await
        .wrap_err(format!("failed to decode src_url to bytes: {url}"))?
        .to_vec();
    let img = ImageData { bytes, path };

    Ok(img)
}

async fn extract_counter(c: &Client) -> Result<u16, MainError> {
    let pgs_elm = c
        .wait()
        .at_most(Duration::from_millis(5000))
        .for_element(Locator::Css("span.hoz-total-image"))
        .await?;

    let text = pgs_elm.html(true).await?;

    text.parse::<u16>()
        .map_err(|_| MainError::ParseCounterElement(text))
}

pub fn write_img(data: ImageData) -> Result<()> {
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
            .expect("error launching magick")
            .wait()?;

        print!("\r{i}");
    }

    Ok(())
}

pub fn gen_rand() -> i32 {
    let num = vec![2, 3, 50, 80, 23124];
    let add = &num as *const Vec<i32>;
    add as i32
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

pub async fn _handle_popup(c: &Client) -> Result<(), MainError> {
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
