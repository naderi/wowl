use anyhow::{anyhow, Result};
use rand::seq::SliceRandom;
use rand::Rng;
use reqwest::Client;
use serde::Serialize;

use crate::config::Settings;

/// A single photo returned by a provider, with everything the UI needs to show
/// credit and everything the backend needs to download the full image.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub id: String,
    pub provider: String,
    pub width: u32,
    pub height: u32,
    pub photographer: String,
    pub photographer_url: Option<String>,
    /// Link to the photo's page on the source platform.
    pub source_url: Option<String>,
    /// Direct URL of the full-size image to download.
    pub image_url: String,
    /// Endpoint to ping once the image has been downloaded (Unsplash ToS).
    pub download_trigger: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Picsum,
    Unsplash,
    Bing,
    Pixabay,
    Wallhaven,
}

impl Provider {
    pub fn from_id(id: &str) -> Provider {
        match id {
            "unsplash" => Provider::Unsplash,
            "bing" => Provider::Bing,
            "pixabay" => Provider::Pixabay,
            "wallhaven" => Provider::Wallhaven,
            _ => Provider::Picsum,
        }
    }

    #[allow(dead_code)] // handy for logging / round-tripping
    pub fn id(&self) -> &'static str {
        match self {
            Provider::Picsum => "picsum",
            Provider::Unsplash => "unsplash",
            Provider::Bing => "bing",
            Provider::Pixabay => "pixabay",
            Provider::Wallhaven => "wallhaven",
        }
    }

    pub fn supports_search(&self) -> bool {
        matches!(
            self,
            Provider::Unsplash | Provider::Pixabay | Provider::Wallhaven
        )
    }

    /// Fetch metadata for one image. `target` is the desired pixel size
    /// (usually the primary monitor resolution).
    pub async fn fetch(
        &self,
        client: &Client,
        settings: &Settings,
        target: (u32, u32),
    ) -> Result<Photo> {
        let terms = settings.terms();
        match self {
            Provider::Picsum => fetch_picsum(client, target).await,
            Provider::Unsplash => {
                fetch_unsplash(client, &terms, settings.unsplash_key.trim(), target).await
            }
            Provider::Bing => fetch_bing(client, &settings.language, target).await,
            Provider::Pixabay => {
                fetch_pixabay(client, &terms, settings.pixabay_key.trim()).await
            }
            Provider::Wallhaven => {
                fetch_wallhaven(client, &terms, settings.wallhaven_key.trim(), target).await
            }
        }
    }
}

const UTM: &str = "utm_source=Wowl&utm_medium=referral";

/// Append the Unsplash referral UTM parameters to any link that points at
/// unsplash.com (required by the Unsplash API guidelines). Links to other hosts
/// are returned unchanged.
fn unsplash_utm(url: &str) -> String {
    if url.contains("://unsplash.com/") || url.contains("://www.unsplash.com/") {
        let sep = if url.contains('?') { '&' } else { '?' };
        format!("{url}{sep}{UTM}")
    } else {
        url.to_string()
    }
}

#[derive(serde::Deserialize)]
struct PicsumItem {
    id: String,
    author: String,
    #[allow(dead_code)]
    width: u32,
    #[allow(dead_code)]
    height: u32,
    /// Photo page (points at the original on Unsplash).
    url: String,
}

async fn fetch_picsum(client: &Client, target: (u32, u32)) -> Result<Photo> {
    let page = rand::thread_rng().gen_range(1..=10);
    let list: Vec<PicsumItem> = client
        .get(format!("https://picsum.photos/v2/list?page={page}&limit=30"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let item = {
        let mut rng = rand::thread_rng();
        list.choose(&mut rng)
            .ok_or_else(|| anyhow!("Picsum returned no images"))?
    };

    let (w, h) = target;
    // Picsum's `url` points at the original photo page on Unsplash — add the
    // referral UTM params the Unsplash guidelines ask for.
    let page = unsplash_utm(&item.url);
    Ok(Photo {
        id: format!("picsum-{}", item.id),
        provider: "picsum".into(),
        width: w,
        height: h,
        photographer: item.author.clone(),
        photographer_url: Some(page.clone()),
        source_url: Some(page),
        image_url: format!("https://picsum.photos/id/{}/{}/{}", item.id, w, h),
        download_trigger: None,
    })
}

/* ===================== Unsplash ===================== */

#[derive(serde::Deserialize)]
struct UnsplashPhoto {
    id: String,
    urls: UnsplashUrls,
    links: UnsplashLinks,
    user: UnsplashUser,
}
#[derive(serde::Deserialize)]
struct UnsplashUrls {
    raw: String,
}
#[derive(serde::Deserialize)]
struct UnsplashLinks {
    html: String,
    download_location: Option<String>,
}
#[derive(serde::Deserialize)]
struct UnsplashUser {
    name: String,
    links: Option<UnsplashUserLinks>,
}
#[derive(serde::Deserialize)]
struct UnsplashUserLinks {
    html: String,
}

async fn fetch_unsplash(
    client: &Client,
    terms: &[String],
    key: &str,
    target: (u32, u32),
) -> Result<Photo> {
    let key = key.trim();
    if key.is_empty() {
        return Err(anyhow!(
            "Unsplash access key missing — add it in the settings"
        ));
    }
    let (w, h) = target;

    let mut req = client
        .get("https://api.unsplash.com/photos/random")
        .header("Authorization", format!("Client-ID {key}"))
        .query(&[("orientation", "landscape"), ("content_filter", "high")]);
    if !terms.is_empty() {
        req = req.query(&[("query", terms.join(" "))]);
    }

    let resp = req.send().await?;
    match resp.status() {
        reqwest::StatusCode::UNAUTHORIZED => {
            return Err(anyhow!("Unsplash rejected the access key"))
        }
        reqwest::StatusCode::FORBIDDEN => {
            return Err(anyhow!("Unsplash rate limit reached — try again later"))
        }
        _ => {}
    }
    let u: UnsplashPhoto = resp.error_for_status()?.json().await?;

    let image_url = format!("{}&w={w}&h={h}&fit=crop&q=85&fm=jpg", u.urls.raw);

    Ok(Photo {
        id: format!("unsplash-{}", u.id),
        provider: "unsplash".into(),
        width: w,
        height: h,
        photographer: u.user.name,
        photographer_url: u.user.links.map(|l| format!("{}?{UTM}", l.html)),
        source_url: Some(format!("{}?{UTM}", u.links.html)),
        image_url,
        download_trigger: u.links.download_location,
    })
}

/* ===================== Bing – image of the day ===================== */

#[derive(serde::Deserialize)]
struct BingResponse {
    images: Vec<BingImage>,
}
#[derive(serde::Deserialize)]
struct BingImage {
    urlbase: String,
    copyright: String,
    #[serde(default)]
    copyrightlink: String,
}

async fn fetch_bing(client: &Client, lang: &str, target: (u32, u32)) -> Result<Photo> {
    let mkt = if lang == "de" { "de-DE" } else { "en-US" };
    let idx = rand::thread_rng().gen_range(0..8).to_string();

    let resp: BingResponse = client
        .get("https://www.bing.com/HPImageArchive.aspx")
        .query(&[
            ("format", "js"),
            ("idx", idx.as_str()),
            ("n", "1"),
            ("mkt", mkt),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let img = resp
        .images
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Bing returned no image"))?;

    let base = img.urlbase.trim_start_matches('/');
    let image_url = format!("https://www.bing.com/{base}_UHD.jpg");

    // "Caption (© Photographer/Agency)" → keep the part after "(©".
    let photographer = img
        .copyright
        .rsplit_once("(©")
        .map(|(_, rest)| rest.trim().trim_end_matches(')').trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| img.copyright.clone());

    let id = img
        .urlbase
        .split("id=OHR.")
        .nth(1)
        .and_then(|s| s.split(['_', '&']).next())
        .unwrap_or("day");

    let (w, h) = target;
    Ok(Photo {
        id: format!("bing-{id}"),
        provider: "bing".into(),
        width: w,
        height: h,
        photographer,
        photographer_url: None,
        source_url: Some(if img.copyrightlink.is_empty() {
            "https://www.bing.com".into()
        } else {
            img.copyrightlink
        }),
        image_url,
        download_trigger: None,
    })
}

/* ===================== Pixabay ===================== */

#[derive(serde::Deserialize)]
struct PixabayResponse {
    hits: Vec<PixabayHit>,
}
#[derive(serde::Deserialize)]
struct PixabayHit {
    id: u64,
    #[serde(rename = "pageURL")]
    page_url: String,
    #[serde(rename = "largeImageURL")]
    large_image_url: String,
    /// Only present for accounts with full API access; fall back to the large URL.
    #[serde(rename = "fullHDURL", default)]
    full_hd_url: Option<String>,
    #[serde(rename = "imageURL", default)]
    image_url: Option<String>,
    user: String,
    user_id: u64,
}

async fn fetch_pixabay(client: &Client, terms: &[String], key: &str) -> Result<Photo> {
    if key.is_empty() {
        return Err(anyhow!("Pixabay API key missing — add it in the settings"));
    }

    let mut query: Vec<(&str, String)> = vec![
        ("key", key.to_string()),
        ("image_type", "photo".into()),
        ("orientation", "horizontal".into()),
        ("safesearch", "true".into()),
        ("per_page", "200".into()),
    ];
    if !terms.is_empty() {
        query.push(("q", terms.join(" ")));
    }

    let resp = client
        .get("https://pixabay.com/api/")
        .query(&query)
        .send()
        .await?;
    match resp.status() {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::BAD_REQUEST => {
            return Err(anyhow!("Pixabay rejected the API key"))
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            return Err(anyhow!("Pixabay rate limit reached — try again later"))
        }
        _ => {}
    }
    let body: PixabayResponse = resp.error_for_status()?.json().await?;

    let item = {
        let mut rng = rand::thread_rng();
        body.hits
            .choose(&mut rng)
            .ok_or_else(|| anyhow!("Pixabay returned no images"))?
    };

    let image_url = item
        .full_hd_url
        .clone()
        .or_else(|| item.image_url.clone())
        .unwrap_or_else(|| item.large_image_url.clone());

    Ok(Photo {
        id: format!("pixabay-{}", item.id),
        provider: "pixabay".into(),
        width: 0,
        height: 0,
        photographer: item.user.clone(),
        photographer_url: Some(format!(
            "https://pixabay.com/users/{}-{}/",
            item.user, item.user_id
        )),
        source_url: Some(item.page_url.clone()),
        image_url,
        download_trigger: None,
    })
}

/* ===================== Wallhaven ===================== */

#[derive(serde::Deserialize)]
struct WallhavenSearch {
    data: Vec<WallhavenItem>,
}
#[derive(serde::Deserialize)]
struct WallhavenItem {
    id: String,
    /// Wallpaper page, e.g. `https://wallhaven.cc/w/<id>`.
    url: String,
    /// Direct full-image URL.
    path: String,
}
#[derive(serde::Deserialize)]
struct WallhavenDetailResponse {
    data: WallhavenDetail,
}
#[derive(serde::Deserialize)]
struct WallhavenDetail {
    uploader: Option<WallhavenUploader>,
}
#[derive(serde::Deserialize)]
struct WallhavenUploader {
    username: String,
}

async fn fetch_wallhaven(
    client: &Client,
    terms: &[String],
    key: &str,
    target: (u32, u32),
) -> Result<Photo> {
    let (w, h) = target;
    let atleast = format!("{}x{}", w.min(1920), h.min(1080));
    let mut query: Vec<(&str, String)> = vec![
        ("categories", "100".into()), // general only
        ("purity", "100".into()),     // SFW only
        ("sorting", "random".into()),
        ("ratios", "landscape".into()),
        ("atleast", atleast),
    ];
    if !terms.is_empty() {
        query.push(("q", terms.join(" ")));
    }
    if !key.is_empty() {
        query.push(("apikey", key.to_string()));
    }

    let resp = client
        .get("https://wallhaven.cc/api/v1/search")
        .query(&query)
        .send()
        .await?;
    match resp.status() {
        reqwest::StatusCode::UNAUTHORIZED => {
            return Err(anyhow!("Wallhaven rejected the API key"))
        }
        reqwest::StatusCode::TOO_MANY_REQUESTS => {
            return Err(anyhow!("Wallhaven rate limit reached — try again later"))
        }
        _ => {}
    }
    let search: WallhavenSearch = resp.error_for_status()?.json().await?;

    let item = {
        let mut rng = rand::thread_rng();
        search
            .data
            .choose(&mut rng)
            .ok_or_else(|| anyhow!("Wallhaven returned no wallpapers"))?
    };

    // Search results carry no uploader — a second call to the per-wallpaper
    // endpoint fills it in for attribution. Failure here is non-fatal.
    let mut photographer = String::new();
    let mut photographer_url = None;
    let detail_url = format!("https://wallhaven.cc/api/v1/w/{}", item.id);
    let mut detail_req = client.get(&detail_url);
    if !key.is_empty() {
        detail_req = detail_req.query(&[("apikey", key)]);
    }
    if let Ok(resp) = detail_req.send().await {
        if let Ok(detail) = resp.json::<WallhavenDetailResponse>().await {
            if let Some(u) = detail.data.uploader {
                photographer_url = Some(format!("https://wallhaven.cc/user/{}", u.username));
                photographer = u.username;
            }
        }
    }

    Ok(Photo {
        id: format!("wallhaven-{}", item.id),
        provider: "wallhaven".into(),
        width: 0,
        height: 0,
        photographer,
        photographer_url,
        source_url: Some(item.url.clone()),
        image_url: item.path.clone(),
        download_trigger: None,
    })
}
