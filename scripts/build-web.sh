#!/bin/sh
# WASM 빌드 → web/ekd5_web.wasm. 데이터는 web/data (원본에서 `ekd5 extract`로 만든 폴더 링크)
set -e
cd "$(dirname "$0")/.."
. "$HOME/.cargo/env" 2>/dev/null || true
cargo build --release --target wasm32-unknown-unknown -p ekd5-web
cp target/wasm32-unknown-unknown/release/ekd5_web.wasm web/
[ -e web/data ] || ln -s ../data web/data
ls -la web/ekd5_web.wasm
