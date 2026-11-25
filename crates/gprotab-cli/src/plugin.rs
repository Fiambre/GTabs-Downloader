//! Modo plugin: `gprotab plugin <orden> ...`. Lo usan aplicaciones (p. ej. NeoGuitar) que ejecutan este
//! programa como proceso aparte. Cada orden escribe UNA línea de JSON en stdout y termina con código 0;
//! si falla, escribe `{"error":"..."}` y termina con código 1. Los mensajes de log van a stderr.
//!
//! Protocolo (versión 1):
//! - `plugin info`                      -> `{"protocol":1,"name":"gprotab","version":"0.1.0","site":"gprotab.net"}`
//! - `plugin search <texto> [pagina]`   -> `{"results":[{"id":"nirvana/polly","artist":"Nirvana","title":"Polly"}],"has_next":true}`
//! - `plugin download <id> <carpeta>`   -> `{"path":"C:\\...\\nirvana-polly.gp5"}`
//!
//! `id` es `artista/cancion` (los slugs del sitio). `pagina` empieza en 1.

use gprotab::{Client, Tab};
use serde_json::{json, Value};

pub const PROTOCOL: u32 = 1;

/// Ejecuta una orden del plugin y devuelve el JSON a imprimir (`Err` = mensaje de error).
pub async fn run(args: &[&str]) -> Result<Value, String> {
    match args {
        ["info"] => Ok(json!({
            "protocol": PROTOCOL,
            "name": "gprotab",
            "version": env!("CARGO_PKG_VERSION"),
            "site": "gprotab.net",
        })),
        ["search", q, rest @ ..] => {
            let page = rest.first().and_then(|s| s.parse().ok()).unwrap_or(1u32).max(1);
            let p = Client::new().search_songs_page(q, page).await.map_err(|e| e.to_string())?;
            let results: Vec<Value> = p
                .items
                .iter()
                .map(|t| json!({ "id": id_of(t), "artist": t.artist, "title": t.title }))
                .collect();
            Ok(json!({ "results": results, "has_next": p.has_next }))
        }
        ["download", id, dir] => {
            let (artist, song) = split_id(id).ok_or_else(|| format!("id inválido: {id}"))?;
            let path = Client::new()
                .download(&Tab::from_slugs(artist, song), dir)
                .await
                .map_err(|e| e.to_string())?;
            Ok(json!({ "path": path.to_string_lossy() }))
        }
        _ => Err("uso: plugin info | plugin search <texto> [pagina] | plugin download <id> <carpeta>".into()),
    }
}

fn id_of(t: &Tab) -> String {
    format!("{}/{}", t.artist_slug, t.slug)
}

/// `nirvana/polly` -> (`nirvana`, `polly`). Rechaza vacíos y barras de más (el id acaba en una URL).
fn split_id(id: &str) -> Option<(&str, &str)> {
    let (a, s) = id.split_once('/')?;
    let ok = |x: &str| !x.is_empty() && !x.contains('/') && !x.contains("..") && !x.contains('\\');
    (ok(a) && ok(s)).then_some((a, s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert_eq!(split_id("nirvana/polly"), Some(("nirvana", "polly")));
        assert_eq!(split_id("nirvana"), None);
        assert_eq!(split_id("a/b/c"), None);
        assert_eq!(split_id("../x"), None);
        assert_eq!(split_id("/x"), None);
    }

    #[tokio::test]
    async fn info_and_usage() {
        let v = run(&["info"]).await.unwrap();
        assert_eq!(v["protocol"], PROTOCOL);
        assert!(run(&["nope"]).await.is_err());
        assert!(run(&["download", "mal", "."]).await.is_err());
    }
}
