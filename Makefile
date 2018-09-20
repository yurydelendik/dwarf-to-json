TARGET_WASM_FILE = ./target/wasm32-unknown-unknown/release/dwarf_to_json.wasm
SOURCE_FILES = Cargo.toml $(wildcard src/*.rs)
OTHER_FILES_TO_PACK = misc/package.json misc/index.js misc/cli.js
default: build

build: $(TARGET_WASM_FILE)

$(TARGET_WASM_FILE): $(SOURCE_FILES)
	cargo +nightly build --target=wasm32-unknown-unknown --lib --release

pack: build $(OTHER_FILES_TO_PACK)
	-rm -rf pkg/
	mkdir pkg/
	cp $(TARGET_WASM_FILE) $(OTHER_FILES_TO_PACK) pkg/

clean:
	rm -rf pkg/
	cargo clean

publish: pack
	cd pkg/; npm publish

.PHONY: default build pack publish clean
