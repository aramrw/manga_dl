mod error;
mod test;

use std::{
    fs::{read_dir, File},
    io::{self, Write},
    process::{Child, Command},
    thread::sleep,
    time::{Duration, Instant},
};

use error::MainError;
use fantoccini::{elements::Element, wd::Capabilities, Client, Locator};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), MainError> {
    let start = Instant::now();
    let _ = std::fs::create_dir("download");
    let mut child = start_gd().expect("failed to start gecko driver");
    let c = start_client().await.expect("failed to start fantoccini");

    let manga_v = get_manga_input().expect("failed to get user input");
    c.goto(&manga_v).await?;

    // First click
    let btn = c
        .wait()
        .for_element(Locator::Css("a.rtl-row:nth-child(2)"))
        .await?;
    btn.click().await?;

    // Handle the ad by closing the newly opened tab
    let handles = c.windows().await?;
    if handles.len() > 1 {
        for handle in handles.iter().skip(1) {
            c.switch_to_window(handle.clone()).await?;
            c.close_window().await?;
        }
        c.switch_to_window(handles[0].clone()).await?;
    }

    // Second click
    btn.click().await?;

    println!("\nbeginning download...");

    let ctr = extract_counter(&c).await? - 1;

    println!("beginning loop...");
    for i in 0..ctr {
        download_panel_canvas(i, &c).await?;
        c.execute("hozNextImage()", vec![]).await?;
        print!("\r{}/{ctr}", i + 1);
        std::io::stdout().flush().unwrap();
    }

    let elapsed = start.elapsed();
    println!("\nDownload complete in {:?}", elapsed);

    //c.close().await?;
    child.kill().expect("failed to kill gecko driver.");
    Ok(())
}

async fn download_img(index: u16, c: &Client) -> Result<(), MainError> {
    if let Ok(e) = c
        .wait()
        .at_most(Duration::from_millis(2000))
        .for_element(Locator::Css(
            "div.ds-item.active:nth-child(2) > div:nth-child(1) > canvas:nth-child(2)",
        ))
        .await
    {
        if let Some(img_url) = e.attr("src").await? {
            let res = reqwest::get(img_url).await?;
            let bytes = res.bytes().await?.to_vec();
            let mut file = File::create(format!("download/{index}.png"))
                .expect("Failed to create screenshot file");
            file.write_all(&bytes)
                .expect("Failed to write screenshot data");
        }
    }

    Ok(())
}

async fn handle_popup(c: &Client) -> Result<(), MainError> {
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
        //.capabilities(caps)
        .connect("http://localhost:4444")
        .await
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

async fn download_panel_canvas(index: u16, c: &Client) -> Result<(), MainError> {
    let index = index + 1;

    if let Ok(canvas) = c
        .wait()
        .at_most(Duration::from_millis(2500))
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
            let mut file = File::create(format!("download/{index}.png"))
                .expect("Failed to create screenshot file");
            file.write_all(&ss)
                .expect("Failed to write screenshot data");
        } else {
            println!("Canvas did not load in time.");
        }
    } else {
        download_img(index, c).await?;
    }

    Ok(())
}

pub fn crop_imgs() -> Result<(), Box<dyn std::error::Error>> {
    for (i, entry) in read_dir("download")?.enumerate() {
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

fn start_gd() -> Result<Child, io::Error> {
    let child = Command::new("./geckodriver").spawn()?;
    Ok(child)
}

fn get_manga_input() -> Result<String, io::Error> {
    let mut input = String::new();
    println!("\nEnter mangareader volume url.");
    println!("Example: https://mangareader.to/read/tokyo-ghoul-108/en/volume-1");
    io::stdin().read_line(&mut input)?;

    Ok(input)
}
