export WASI_SDK_PATH=~/wasi-sdk-22.0/
source ~/.cargo/env
export PATH=~/.cargo/bin/:$PATH
cargo build --target wasm32-wasip1 --no-default-features --profile release
cp -f ./target/wasm32-wasip1/release/uiua.wasm ./kotlin/lib/src/main/resources/uiua.wasm
