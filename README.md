# WebAssembly DWARF information into JSON converter

The JSON format is in (JavaScript) source maps format -- the `.debug_line` content.

It is planned to implement serialize and extends the JSON with one additional field: `x-scopes`. See info at https://gist.github.com/yurydelendik/802f36983d50cedb05f984d784dc5159 and https://gist.github.com/yurydelendik/10f3c99879e9459259a6aaf79f39215c.

# Compiling

```
rustup target add wasm32-unknown-unknown
cargo build --target=wasm32-unknown-unknown --lib --release
```

Firefox DevTools uses the resulting
`./target/wasm32-unknown-unknown/release/dwarf_to_json.wasm`, which lives in
mozilla-central at
`devtools/client/shared/source-map-loader/wasm-dwarf/dwarf_to_json.wasm`.

A native command line tool is also available:

```
cargo run --release --bin dwarf-to-json -- input.wasm -o output.map
```
