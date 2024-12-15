use std::{collections::HashSet, io::Write, time::Instant};

use color_eyre::{eyre::Context, Result};
use fantoccini::{Client, Locator};
use spinners::{Spinner, Spinners};
use tokio::time::sleep;

use crate::{
    cli::{Cli, LogLevel, Url},
    g_handle_popup,
    loading::{downloading_panel_data_msg, print_download_complete_msg},
    mangareader::{write_img, ImageData},
    setup_nav,
};

pub type NavigateGroup = (String, String, Spinner);

pub async fn dl_mangagun(client: &Client, url: &Url, args: &Cli) -> Result<()> {
    let (_, dl_path, mut sp) = setup_nav(client, url, args).await?;
    let start = Instant::now();

    // hide the top-navbar
    execute_set_element_hidden_inline(client, ".navbar").await?;
    // hide the bottom-navbar
    execute_set_element_hidden_inline(client, "#rd-side_icon").await?;

    let index_map: Option<HashSet<&usize>> =
        args.indexes.as_ref().map(|slice| slice.iter().collect());
    let img_data = get_all_images(&dl_path, client, &mut sp, index_map, &args.log).await?;

    let elapsed = start.elapsed();
    print_download_complete_msg(elapsed);

    //sp = Spinner::new(Spinners::Triangle, "".into());

    img_data
        .into_iter()
        .for_each(|data| write_img(&data).unwrap());
    //sp.stop_with_newline();

    Ok(())
}

async fn get_all_images(
    dl_path: &str,
    c: &Client,
    sp: &mut Spinner,
    index_map: Option<HashSet<&usize>>,
    log: &LogLevel,
) -> Result<HashSet<ImageData>> {
    g_handle_popup(c).await.wrap_err(line!())?;
    let imgs = c.find_all(Locator::Css("img.chapter-img")).await?;
    let mut new_imgs: HashSet<ImageData> = HashSet::new();

    let max = imgs.len();
    // if the index map is Some, skip indexes that aren't specified
    for (i, img) in imgs.into_iter().enumerate() {
        if let Some(i_map) = &index_map {
            if !i_map.contains(&i) {
                continue;
            }
        }
        execute_set_element_hidden_inline(c, "#adModal").await?;
        execute_set_element_hidden_computed(c).await?;

        // wait until the image src is not the loading GIF
        while let Some(src) = img.attr("src").await? {
            if !src.contains("gif") {
                let rect = img.rectangle().await?;
                // the spinner gif is 100 x 99.9...
                if rect.2 > 300.0 {
                    match log {
                        LogLevel::Full => {
                            print!(":\n  {src}");
                            std::io::stdout().flush().expect("failed to flush output");
                        }
                        _ => { /* skip */ }
                    }
                    let msg = downloading_panel_data_msg(i as u16, max as u16);
                    *sp = Spinner::new(Spinners::Arc, msg);
                }
                break;
            }
            sleep(std::time::Duration::from_millis(300)).await;
        }

        let bytes = img.screenshot().await?;
        let path = format!("{dl_path}/{i}.jpg");
        let img = ImageData { bytes, path };

        new_imgs.insert(img);
    }

    Ok(new_imgs)
}

pub async fn execute_set_element_hidden_computed(c: &Client) -> Result<()> {
    let script = r#"
    var elements = document.querySelectorAll('*');
    elements.forEach(function(element) {
    var style = window.getComputedStyle(element);
    if (style.zIndex === '2147483647') {
        element.style.display = 'none';
    }});
    "#;
    c.execute(script, vec![]).await?;
    Ok(())
}

/// * selector
///
/// ".navbar" || "#navbar";
pub async fn execute_set_element_hidden_inline(c: &Client, selector: &str) -> Result<()> {
    let script = format!(
        r#"
            var element = document.querySelector('{}');
            if (element) {{
                element.style.display = 'none';
            }} 
            "#,
        selector
    );
    c.execute(&script, vec![]).await?;
    Ok(())
}
