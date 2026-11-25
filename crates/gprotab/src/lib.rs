//! Cliente async para buscar y descargar tabs de Guitar Pro desde gprotab.net.
//!
//! ```no_run
//! # async fn demo() -> gprotab::Result<()> {
//! use gprotab::Client;
//!
//! let client = Client::new();
//! // Hasta 2 páginas de resultados (20 por página).
//! let songs = client.search_songs("polly", 2).await?;
//! let info = client.tab_info(&songs[0]).await?;
//! println!("{} ({})", info.song, info.file_type);
//!
//! // Descarga varios tabs, 4 a la vez.
//! let results = client.download_many(&songs, "./tabs", 4).await;
//! # Ok(()) }
//! ```
//!
//! Funciona sobre cualquier runtime de tokio. Con el feature `serde` los tipos
//! públicos implementan `Serialize`/`Deserialize`.
//!
//! Flujo del sitio (sacado del HAR):
//! - Búsqueda:  `GET /en/search?type=artist|song&q=...[&page=N]` (20 por página)
//! - Artista:   `GET /en/tabs/{artist}`            (lista de tabs)
//! - Tab:       `GET /en/tabs/{artist}/{song}`     (metadatos)
//! - Descarga:  `GET /en/tabs/{artist}/{song}?download` (attachment, nombre en Content-Disposition)

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::stream::{self, StreamExt};
use scraper::{Html, Selector};
use url::Url;

const BASE: &str = "https://gprotab.net";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/157.0.0.0 Safari/537.36";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("url: {0}")]
    Url(#[from] url::ParseError),
    #[error("respuesta inesperada: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Artist {
    pub name: String,
    pub slug: String,
}

/// Un tab (canción) de un artista.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tab {
    pub artist: String,
    pub artist_slug: String,
    pub title: String,
    pub slug: String,
}

impl Tab {
    /// Construye un tab solo con los slugs (suficiente para `tab_info` y `download`).
    pub fn from_slugs(artist_slug: &str, slug: &str) -> Self {
        Self {
            artist: artist_slug.into(),
            artist_slug: artist_slug.into(),
            title: slug.into(),
            slug: slug.into(),
        }
    }
    pub fn page_url(&self) -> String {
        format!("{BASE}/en/tabs/{}/{}", self.artist_slug, self.slug)
    }
    pub fn download_url(&self) -> String {
        format!("{}?download", self.page_url())
    }
}

/// Metadatos de la página de un tab.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TabInfo {
    pub band: String,
    pub song: String,
    pub file_type: String,
    pub file_size: String,
    pub downloads: Option<u32>,
    pub rating: String,
}

/// Una página de resultados de búsqueda.
#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// `true` si el sitio enlaza a la página siguiente.
    pub has_next: bool,
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
    delay: Duration,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    pub fn new() -> Self {
        Self::with_base(BASE)
    }

    pub fn with_base(base: &str) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(UA)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("cliente http");
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            delay: Duration::from_millis(250),
        }
    }

    /// Pausa que se espera antes de cada petición de `download_many` y de la
    /// paginación (por defecto 250 ms). Es cortesía con el servidor.
    pub fn delay(mut self, d: Duration) -> Self {
        self.delay = d;
        self
    }

    async fn get_text(&self, url: &str) -> Result<String> {
        Ok(self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }

    fn search_url(&self, kind: &str, q: &str, page: u32) -> Result<Url> {
        let mut params = vec![("type", kind.to_string()), ("q", q.to_string())];
        if page > 1 {
            params.push(("page", page.to_string()));
        }
        Ok(Url::parse_with_params(
            &format!("{}/en/search", self.base),
            &params,
        )?)
    }

    /// Una página de resultados de artistas (`page` empieza en 1).
    pub async fn search_artists_page(&self, q: &str, page: u32) -> Result<Page<Artist>> {
        let body = self
            .get_text(self.search_url("artist", q, page)?.as_str())
            .await?;
        Ok(parse_artists(&body, page))
    }

    /// Una página de resultados de canciones (`page` empieza en 1).
    pub async fn search_songs_page(&self, q: &str, page: u32) -> Result<Page<Tab>> {
        let body = self
            .get_text(self.search_url("song", q, page)?.as_str())
            .await?;
        Ok(parse_songs(&body, page))
    }

    /// Busca artistas, siguiendo la paginación hasta `max_pages` páginas.
    pub async fn search_artists(&self, q: &str, max_pages: u32) -> Result<Vec<Artist>> {
        let mut out = Vec::new();
        for page in 1..=max_pages.max(1) {
            if page > 1 {
                tokio::time::sleep(self.delay).await;
            }
            let p = self.search_artists_page(q, page).await?;
            out.extend(p.items);
            if !p.has_next {
                break;
            }
        }
        Ok(out)
    }

    /// Busca canciones, siguiendo la paginación hasta `max_pages` páginas.
    pub async fn search_songs(&self, q: &str, max_pages: u32) -> Result<Vec<Tab>> {
        let mut out = Vec::new();
        for page in 1..=max_pages.max(1) {
            if page > 1 {
                tokio::time::sleep(self.delay).await;
            }
            let p = self.search_songs_page(q, page).await?;
            out.extend(p.items);
            if !p.has_next {
                break;
            }
        }
        Ok(out)
    }

    /// Lista todos los tabs de un artista (por slug, ej. `nirvana`).
    pub async fn artist_tabs(&self, artist_slug: &str) -> Result<Vec<Tab>> {
        let body = self
            .get_text(&format!("{}/en/tabs/{artist_slug}", self.base))
            .await?;
        Ok(parse_artist_tabs(&body, artist_slug))
    }

    /// Metadatos de un tab (tipo de archivo, tamaño, descargas, rating).
    pub async fn tab_info(&self, tab: &Tab) -> Result<TabInfo> {
        let url = tab.page_url().replacen(BASE, &self.base, 1);
        let body = self.get_text(&url).await?;
        parse_tab_info(&body)
            .ok_or_else(|| Error::Parse(format!("no se encontró la tabla de datos en {url}")))
    }

    /// Descarga el tab dentro de `dir` y devuelve la ruta escrita.
    /// El nombre sale del header Content-Disposition (ej. `nirvana-new_wave_polly.gpx`).
    pub async fn download(&self, tab: &Tab, dir: impl AsRef<Path>) -> Result<PathBuf> {
        let url = tab.download_url().replacen(BASE, &self.base, 1);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let name = resp
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(filename_from_disposition)
            .ok_or_else(|| Error::Parse("falta Content-Disposition (¿tab inexistente?)".into()))?;
        let bytes = resp.bytes().await?;
        tokio::fs::create_dir_all(dir.as_ref()).await?;
        let path = dir.as_ref().join(sanitize(&name));
        tokio::fs::write(&path, &bytes).await?;
        Ok(path)
    }

    /// Descarga varios tabs con como máximo `concurrency` peticiones en vuelo.
    /// Cada petición espera `delay` antes de salir. Devuelve un resultado por tab,
    /// en el mismo orden que `tabs`; un fallo no cancela el resto.
    pub async fn download_many(
        &self,
        tabs: &[Tab],
        dir: impl AsRef<Path>,
        concurrency: usize,
    ) -> Vec<(Tab, Result<PathBuf>)> {
        let dir = dir.as_ref().to_path_buf();
        let mut results: Vec<_> = stream::iter(tabs.iter().cloned().enumerate())
            .map(|(i, tab)| {
                let (c, dir) = (self.clone(), dir.clone());
                async move {
                    tokio::time::sleep(c.delay).await;
                    let r = c.download(&tab, &dir).await;
                    (i, tab, r)
                }
            })
            .buffer_unordered(concurrency.max(1))
            .collect()
            .await;
        results.sort_by_key(|(i, _, _)| *i);
        results.into_iter().map(|(_, t, r)| (t, r)).collect()
    }
}

// ---- parsing (síncrono: `Html` no es `Send`, así que nunca cruza un `.await`) ----

fn parse_artists(body: &str, page: u32) -> Page<Artist> {
    let doc = Html::parse_document(body);
    let s = sel("ol.artists li a");
    let items = doc
        .select(&s)
        .filter_map(|a| {
            let slug = slug_after(a.value().attr("href")?, "/tabs/")?;
            Some(Artist {
                name: text(&a),
                slug,
            })
        })
        .collect();
    Page {
        items,
        has_next: has_next(&doc, page),
    }
}

fn parse_songs(body: &str, page: u32) -> Page<Tab> {
    let doc = Html::parse_document(body);
    let (row, band, name) = (sel("div.search-tab"), sel("a.tab-band"), sel("a.tab-name"));
    let mut items = Vec::new();
    for r in doc.select(&row) {
        let (Some(b), Some(n)) = (r.select(&band).next(), r.select(&name).next()) else {
            continue;
        };
        let artist_slug = b.value().attr("href").and_then(|h| slug_after(h, "/tabs/"));
        let slug = n
            .value()
            .attr("href")
            .and_then(|h| h.rsplit('/').next().map(str::to_string));
        if let (Some(artist_slug), Some(slug)) = (artist_slug, slug) {
            items.push(Tab {
                artist: text(&b),
                artist_slug,
                title: text(&n),
                slug,
            });
        }
    }
    Page {
        items,
        has_next: has_next(&doc, page),
    }
}

fn parse_artist_tabs(body: &str, artist_slug: &str) -> Vec<Tab> {
    let doc = Html::parse_document(body);
    let artist = doc
        .select(&sel("h1"))
        .next()
        .map(|h| text(&h))
        .unwrap_or_else(|| artist_slug.to_string());
    let prefix = format!("/en/tabs/{artist_slug}/");
    doc.select(&sel("ul.tabs li a"))
        .filter_map(|a| {
            let slug = a.value().attr("href")?.strip_prefix(&prefix)?.to_string();
            Some(Tab {
                artist: artist.clone(),
                artist_slug: artist_slug.to_string(),
                title: text(&a),
                slug,
            })
        })
        .collect()
}

fn parse_tab_info(body: &str) -> Option<TabInfo> {
    let doc = Html::parse_document(body);
    let (row, cell) = (sel("div.tab-data tr"), sel("td"));
    let mut info = TabInfo::default();
    for r in doc.select(&row) {
        let cells: Vec<_> = r.select(&cell).collect();
        if cells.len() != 2 {
            continue;
        }
        let v = text(&cells[1]);
        match text(&cells[0]).as_str() {
            "Band" => info.band = v,
            "Song" => info.song = v,
            "Tab file type" => info.file_type = v,
            "File size" => info.file_size = v,
            "Times downloaded" => info.downloads = v.parse().ok(),
            "Rating" => info.rating = v.split_whitespace().collect::<Vec<_>>().join(" "),
            _ => {}
        }
    }
    (!info.song.is_empty()).then_some(info)
}

/// ¿Hay un enlace `page=N+1` en el paginador?
fn has_next(doc: &Html, page: u32) -> bool {
    let needle = format!("page={}", page + 1);
    doc.select(&sel("div.pages a")).any(|a| {
        a.value()
            .attr("href")
            .is_some_and(|h| h.split('&').any(|p| p == needle) || h.ends_with(&needle))
    })
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("selector css válido")
}

fn text(el: &scraper::ElementRef) -> String {
    el.text().collect::<String>().trim().to_string()
}

/// `/en/tabs/nirvana/x` -> `nirvana`
fn slug_after(href: &str, marker: &str) -> Option<String> {
    let rest = href.split(marker).nth(1)?;
    let slug = rest.split('/').next()?;
    (!slug.is_empty()).then(|| slug.to_string())
}

fn filename_from_disposition(v: &str) -> Option<String> {
    let rest = v.split("filename=").nth(1)?;
    Some(rest.trim().trim_matches('"').to_string())
}

/// Evita path traversal y caracteres inválidos en Windows.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition() {
        assert_eq!(
            filename_from_disposition(r#"attachment; filename="nirvana-new_wave_polly.gpx""#)
                .as_deref(),
            Some("nirvana-new_wave_polly.gpx")
        );
    }

    #[test]
    fn slugs() {
        assert_eq!(
            slug_after("/en/tabs/nirvana", "/tabs/").as_deref(),
            Some("nirvana")
        );
        assert_eq!(
            slug_after("/en/tabs/nirvana/polly", "/tabs/").as_deref(),
            Some("nirvana")
        );
    }

    #[test]
    fn sanitizes() {
        assert_eq!(sanitize("../a:b.gp5"), ".._a_b.gp5");
    }

    #[test]
    fn pagination() {
        let html = r#"<div class="search-tab"><a class="tab-band" href="/en/tabs/x">X</a><a class="tab-name" href="/en/tabs/x/y">Y</a></div>
            <div class="pages"><span class="current">1</span><a href="/en/search/?type=song&q=love&page=2">2</a></div>"#;
        let p = parse_songs(html, 1);
        assert_eq!(p.items.len(), 1);
        assert!(p.has_next);
        assert!(!parse_songs(html, 2).has_next);
    }
}
