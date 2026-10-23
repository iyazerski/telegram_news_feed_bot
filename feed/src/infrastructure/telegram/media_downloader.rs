use std::time::Duration;

use common::telegram::bot_api::TelegramUpload;
use reqwest::header::CONTENT_TYPE;

use crate::use_cases::forward_posts::MediaSource;

// Telegram albums hold at most 10 items.
const MEDIA_GROUP_LIMIT: usize = 10;
// The media types the Bot API accepts as photos and videos, with their file extensions.
const SUPPORTED_MEDIA_TYPES: [(&str, &str); 3] = [("image/jpeg", "jpg"), ("image/png", "png"), ("video/mp4", "mp4")];

/// Preview media that cannot become a Bot API upload.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("{0}")]
    Unsupported(String),
    #[error("media download failed: {0}")]
    Http(#[from] reqwest::Error),
}

/// Downloads a post's preview media within one byte allowance for the whole post.
pub struct TelegramMediaDownloader {
    http: reqwest::Client,
    max_bytes: u64,
}

impl TelegramMediaDownloader {
    /// Create a downloader that gives up on connections or reads slower than `timeout`.
    pub fn new(timeout: Duration, max_bytes: u64) -> anyhow::Result<Self> {
        // Large videos take longer than a request timeout to download, so only stalls time out.
        let http = reqwest::Client::builder().connect_timeout(timeout).read_timeout(timeout).build()?;
        Ok(Self { http, max_bytes })
    }
}

impl MediaSource for TelegramMediaDownloader {
    async fn download_media(&self, media_urls: &[String]) -> Result<Vec<TelegramUpload>, MediaError> {
        let mut media = Vec::new();
        let mut downloaded_bytes = 0;
        for (index, media_url) in media_urls.iter().take(MEDIA_GROUP_LIMIT).enumerate() {
            let (content, content_type, extension) =
                self.download_file(media_url, self.max_bytes - downloaded_bytes).await?;
            downloaded_bytes += content.len() as u64;
            media.push(TelegramUpload {
                field_name: format!("media{index}"),
                filename: format!("telegram-media-{index}.{extension}"),
                content,
                content_type: content_type.to_owned(),
            });
        }
        Ok(media)
    }
}

impl TelegramMediaDownloader {
    /// Download one media file of at most `max_bytes`, and return it with its content type and file extension.
    async fn download_file(
        &self,
        media_url: &str,
        max_bytes: u64,
    ) -> Result<(Vec<u8>, &'static str, &'static str), MediaError> {
        let mut response = self.http.get(media_url).send().await?.error_for_status()?;
        let raw_content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| MediaError::Unsupported("Telegram preview media has no content type".to_owned()))?;
        let (content_type, extension) = supported_media_type(raw_content_type)?;
        let too_large = || MediaError::Unsupported("Telegram preview media is too large".to_owned());
        let expected_bytes = response.content_length().unwrap_or(0);
        if expected_bytes > max_bytes {
            return Err(too_large());
        }

        // Reserving the announced size keeps the buffer from doubling past the allowance while it grows.
        let mut content = Vec::with_capacity(usize::try_from(expected_bytes).unwrap_or(0));
        while let Some(chunk) = response.chunk().await? {
            content.extend_from_slice(&chunk);
            if content.len() as u64 > max_bytes {
                return Err(too_large());
            }
        }
        Ok((content, content_type, extension))
    }
}

/// Return the supported media type and file extension for a `Content-Type` header value.
fn supported_media_type(raw_content_type: &str) -> Result<(&'static str, &'static str), MediaError> {
    let content_type = raw_content_type.split(';').next().unwrap_or_default().trim().to_lowercase();
    SUPPORTED_MEDIA_TYPES
        .into_iter()
        .find(|(supported, _)| *supported == content_type)
        .ok_or_else(|| MediaError::Unsupported(format!("Unsupported Telegram preview media type: {content_type}")))
}

#[cfg(test)]
mod tests {
    use common::testing::{CannedResponse, serve};

    use super::*;

    fn downloader(max_bytes: u64) -> TelegramMediaDownloader {
        TelegramMediaDownloader::new(Duration::from_secs(10), max_bytes).unwrap()
    }

    fn jpeg() -> CannedResponse {
        CannedResponse::new(200, "image/jpeg", "data")
    }

    #[tokio::test]
    async fn media_shares_one_allowance_for_the_whole_post() {
        // Two four-byte files fit eight bytes, but not seven.
        for (max_bytes, fits) in [(8, true), (7, false)] {
            let (url, server) = serve(vec![jpeg(), jpeg()]).await;
            let urls = [format!("{url}/one.jpg"), format!("{url}/two.jpg")];

            let media = downloader(max_bytes).download_media(&urls).await;

            if fits {
                let media = media.unwrap();
                assert_eq!(media.iter().map(|item| item.content.as_slice()).collect::<Vec<_>>(), [b"data", b"data"]);
                assert_eq!(media[1].field_name, "media1");
                assert_eq!(media[1].filename, "telegram-media-1.jpg");
                server.await.unwrap();
            } else {
                assert!(matches!(media, Err(MediaError::Unsupported(_))));
            }
        }
    }

    #[test]
    fn only_bot_api_media_types_are_supported() {
        assert_eq!(supported_media_type("Video/MP4; codecs=avc1").unwrap(), ("video/mp4", "mp4"));
        assert!(supported_media_type("image/webp").is_err());
    }
}
