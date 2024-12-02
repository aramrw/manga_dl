use std::{collections::HashSet, time::Instant};

use color_eyre::{owo_colors::OwoColorize, Result};
use fantoccini::{Client, Locator};
use spinners::{Spinner, Spinners};
use tokio::time::sleep;

use crate::{
    args::{Args, Url},
    loading::{downloading_panel_data_msg, print_download_complete_msg, print_indexes_arg},
    mangareader::{gen_rand, write_img, ImageData, _handle_popup},
};

pub async fn dl_mangagun(client: &Client, url: &Url, args: &Args) -> Result<()> {
    let title = url.title.clone().unwrap_or_else(|| gen_rand().to_string());

    let dl_path = match &args.dl_path {
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
    sp.stop_with_newline();

    // hide the navbar so it doesnt interferere with the ss
    client
        .execute(r#"
        const nav = document.querySelector(".navbar");
        if (nav) {
            nav.style.display = 'none';
        }"#,
            vec![],
        )
        .await?;

    // hide the bottom navbar so it does't interfere with the ss
    client
        .execute(r#"
        const bottom_nav = document.getElementById("rd-side_icon");
        if (bottom_nav) {
            bottom_nav.style.display = 'none';
        }"#,
            vec![],
        )
        .await?;


    if let Some(indexes) = &args.indexes {
        print_indexes_arg(indexes);
    }

    let index_map: Option<HashSet<&usize>> =
        args.indexes.as_ref().map(|slice| slice.iter().collect());
    let img_data = get_all_images(&dl_path, client, &mut sp, index_map, args.debug).await?;

    let elapsed = start.elapsed();
    print_download_complete_msg(elapsed);

    sp = Spinner::new(
        Spinners::Triangle,
        "writing data to file..".yellow().to_string(),
    );

    img_data
        .into_iter()
        .for_each(|data| write_img(data).unwrap());
    sp.stop_with_newline();

    Ok(())
}

async fn get_all_images(
    dl_path: &str,
    c: &Client,
    sp: &mut Spinner,
    index_map: Option<HashSet<&usize>>,
    is_debug: bool,
) -> Result<HashSet<ImageData>> {
    _handle_popup(c).await?;
    let imgs = c.find_all(Locator::Css("img.chapter-img")).await?;
    let mut new_imgs: HashSet<ImageData> = HashSet::new();

    let max = imgs.len();
    for (i, img) in imgs.into_iter().enumerate() {
        // if the index map is some skip indexes that aren't specified
        if let Some(i_map) = &index_map {
            if !i_map.contains(&i) {
                continue;
            }
        }
        hide_ad_modal(c).await?;

        // wait until the image src is not the loading GIF
        while let Some(src) = img.attr("src").await? {
            if !src.contains("gif") {
                let rect = img.rectangle().await?;
                // the spinner gif is 100 x 99.999 repeating.
                if rect.2 > 150.0 {
                    is_debug.then(|| println!("\ndl img_src: {src}"));
                    break;
                }
            }
            sleep(std::time::Duration::from_millis(300)).await;
        }

        let bytes = img.screenshot().await?;
        let path = format!("{dl_path}/{i}.jpg");
        let img = ImageData { bytes, path };

        new_imgs.insert(img);

        let msg = downloading_panel_data_msg(i as u16, max as u16);
        if !is_debug {
            *sp = Spinner::new(Spinners::Arc, msg);
        }
    }

    sp.stop_with_newline();
    Ok(new_imgs)
}

pub async fn hide_ad_modal(c: &Client) -> Result<()> {
    c.execute(
        r#"
        const adModal = document.getElementById('adModal');
        if (adModal) {
            adModal.style.display = 'none';  
        }
        "#,
        vec![],
    ).await?;

    Ok(())
}
