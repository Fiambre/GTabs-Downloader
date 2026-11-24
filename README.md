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
