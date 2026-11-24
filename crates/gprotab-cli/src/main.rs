use gprotab::{Client, Tab};

fn usage() -> ! {
    eprintln!(
        "uso:\n  gprotab artist <nombre> [paginas=1]\n  gprotab song <titulo> [paginas=1]\n  gprotab tabs <artist-slug>\n  gprotab info <artist-slug> <song-slug>\n  gprotab get <artist-slug> <song-slug> [dir]\n  gprotab get-all <artist-slug> [dir] [concurrencia=4]"
    );
    std::process::exit(2)
}

#[tokio::main]
async fn main() -> gprotab::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let c = Client::new();
    let n = |s: &str, d: u32| s.parse().unwrap_or(d);
    match a.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["artist", q, rest @ ..] => {
            let pages = rest.first().map_or(1, |s| n(s, 1));
            for x in c.search_artists(q, pages).await? {
                println!("{}\t{}", x.slug, x.name);
            }
        }
        ["song", q, rest @ ..] => {
            let pages = rest.first().map_or(1, |s| n(s, 1));
            for t in c.search_songs(q, pages).await? {
                println!("{}/{}\t{} - {}", t.artist_slug, t.slug, t.artist, t.title);
            }
        }
        ["tabs", art] => {
            for t in c.artist_tabs(art).await? {
                println!("{}\t{}", t.slug, t.title);
            }
        }
        ["info", art, song] => println!("{:#?}", c.tab_info(&Tab::from_slugs(art, song)).await?),
        ["get", art, song] => println!(
            "{}",
            c.download(&Tab::from_slugs(art, song), ".")
                .await?
                .display()
        ),
        ["get", art, song, dir] => println!(
            "{}",
            c.download(&Tab::from_slugs(art, song), dir)
                .await?
                .display()
        ),
        ["get-all", art, rest @ ..] => {
            let dir = rest.first().copied().unwrap_or(art);
            let conc = rest.get(1).map_or(4, |s| n(s, 4)) as usize;
            let tabs = c.artist_tabs(art).await?;
            let (mut ok, mut fail) = (0, 0);
            for (t, r) in c.download_many(&tabs, dir, conc).await {
                match r {
                    Ok(p) => {
                        ok += 1;
                        println!("ok   {}", p.display());
                    }
                    Err(e) => {
                        fail += 1;
                        eprintln!("fail {}: {e}", t.slug);
                    }
                }
            }
            println!("{ok} ok, {fail} fallidos");
        }
        _ => usage(),
    }
    Ok(())
}
