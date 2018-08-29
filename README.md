# WebAssembly DWARF information into JSON converter

The JSON format is in (JavaScript) source maps format -- the `.debug_line` content.

It is planned to implement serialize and extends the JSON with one additional field: `x-scopes`. See info at https://gist.github.com/yurydelendik/802f36983d50cedb05f984d784dc5159 and https://gist.github.com/yurydelendik/10f3c99879e9459259a6aaf79f39215c.

# Compiling

```
cargo +nightly build --target=wasm32-unknown-unknown --lib --release
cp ./target/wasm32-unknown-unknown/release/dwarf_to_json.wasm \
  $DEBUGGER_HTML/assets/wasm/
```
