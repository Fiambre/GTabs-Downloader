# gprotab

Cliente async en Rust para buscar y descargar tabs de Guitar Pro desde gprotab.net.

- `crates/gprotab` — la librería (úsala como dependencia).
- `crates/gprotab-cli` — CLI de ejemplo (`gprotab`).

```toml
[dependencies]
gprotab = { path = "crates/gprotab" }            # o git = "..."
# gprotab = { path = "crates/gprotab", features = ["serde"] }
```

```rust
let client = gprotab::Client::new();
let songs = client.search_songs("polly", 2).await?;
client.download_many(&songs, "./tabs", 4).await;
```

CLI: `cargo run -p gprotab-cli -- song polly 2`, `-- get-all nirvana ./out 4`.

Sé amable con el servidor: `Client::delay` (250 ms por defecto) y la concurrencia limitan la carga.


## Modo plugin (para otras aplicaciones)

`gprotab plugin ...` imprime una línea de JSON por orden y sale con código 0 (o `{"error":"..."}` con código 1).
NeoGuitar lo usa como complemento opcional: baja este programa de las Releases y lo ejecuta como proceso aparte.

```
gprotab plugin info                         {"protocol":1,"name":"gprotab","version":"0.1.0","site":"gprotab.net"}
gprotab plugin search <texto> [pagina]      {"results":[{"id":"nirvana/polly","artist":"Nirvana","title":"Polly"}],"has_next":true}
gprotab plugin download <id> <carpeta>      {"path":"<carpeta>/nirvana-polly.gp3"}
```

Cada Release publica, junto a cada archivo, su `.sha256` para poder verificar la descarga.

Los tabs son obra de sus autores: úsalo para tu práctica personal y respeta los derechos de autor.