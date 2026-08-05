use std::{io::Cursor, path::Path};

use anyhow::{Context, Result, bail};
use image::{ImageFormat, ImageReader};
use tokio::fs;
use uuid::Uuid;

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct SavedUpload {
    pub original_name: String,
    pub file_path: String,
    pub thumb_path: String,
    pub file_size: i64,
    pub mime: String,
    pub width: i32,
    pub height: i32,
}

struct PreparedImage {
    format: ImageFormat,
    thumb: Vec<u8>,
    width: u32,
    height: u32,
}

pub async fn save_image(
    config: &Config,
    board_slug: &str,
    original_name: &str,
    bytes: Vec<u8>,
) -> Result<SavedUpload> {
    if bytes.is_empty() {
        bail!("the uploaded file is empty");
    }
    if bytes.len() > config.max_upload_bytes {
        bail!("the uploaded image is larger than the configured limit");
    }

    let image_bytes = bytes.clone();
    let prepared = tokio::task::spawn_blocking(move || prepare_image(&image_bytes))
        .await
        .context("image worker stopped")??;

    let (extension, mime) = match prepared.format {
        ImageFormat::Jpeg => ("jpg", "image/jpeg"),
        ImageFormat::Png => ("png", "image/png"),
        ImageFormat::Gif => ("gif", "image/gif"),
        ImageFormat::WebP => ("webp", "image/webp"),
        _ => bail!("only JPEG, PNG, GIF, and WebP images are accepted"),
    };

    let id = Uuid::new_v4().simple().to_string();
    let relative_file = format!("{board_slug}/{id}.{extension}");
    let relative_thumb = format!("{board_slug}/{id}s.png");
    let file_path = config.upload_dir.join(&relative_file);
    let thumb_path = config.upload_dir.join(&relative_thumb);
    fs::create_dir_all(file_path.parent().expect("upload has parent")).await?;
    fs::write(&file_path, &bytes).await?;
    if let Err(error) = fs::write(&thumb_path, &prepared.thumb).await {
        let _ = fs::remove_file(&file_path).await;
        return Err(error.into());
    }

    Ok(SavedUpload {
        original_name: safe_display_filename(original_name),
        file_path: relative_file.replace('\\', "/"),
        thumb_path: relative_thumb.replace('\\', "/"),
        file_size: bytes.len() as i64,
        mime: mime.to_owned(),
        width: prepared.width as i32,
        height: prepared.height as i32,
    })
}

pub async fn remove_upload(upload_root: &Path, file: Option<&str>, thumb: Option<&str>) {
    for relative in [file, thumb].into_iter().flatten() {
        if is_safe_relative_path(relative) {
            let _ = fs::remove_file(upload_root.join(relative)).await;
        }
    }
}

fn prepare_image(bytes: &[u8]) -> Result<PreparedImage> {
    let metadata_reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .context("the upload is not a recognized image")?;
    let format = metadata_reader
        .format()
        .context("the upload does not have a recognized image format")?;
    if !matches!(
        format,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP
    ) {
        bail!("only JPEG, PNG, GIF, and WebP images are accepted");
    }
    let (width, height) = metadata_reader
        .into_dimensions()
        .context("the image dimensions could not be read")?;
    if width == 0 || height == 0 || width > 12_000 || height > 12_000 {
        bail!("the image dimensions are not allowed");
    }
    if u64::from(width) * u64::from(height) > 50_000_000 {
        bail!("the image contains too many pixels");
    }
    let image = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .context("the upload is not a recognized image")?
        .decode()
        .context("the image could not be decoded")?;
    let thumbnail = image.thumbnail(250, 250);
    let mut output = Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut output, ImageFormat::Png)
        .context("the thumbnail could not be created")?;
    Ok(PreparedImage {
        format,
        thumb: output.into_inner(),
        width,
        height,
    })
}

fn safe_display_filename(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("image")
        .chars()
        .filter(|character| !character.is_control())
        .take(180)
        .collect::<String>();
    if base.trim().is_empty() {
        "image".to_owned()
    } else {
        base
    }
}

fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with(['/', '\\'])
        && !path.split(['/', '\\']).any(|part| part == "..")
}
