.PHONY: build

# Define target paths relative to the current directory
RUST_LIB_PATH := ./rust_src/target/release/libmylibrary.a

build: build_rust
	go build ./...

build_rust:
	@echo "--> Compiling Rust library in release mode..."
	@cargo build --release
	@echo "--> Rust compilation finished."

clean:
	cargo clean
	@rm -f main