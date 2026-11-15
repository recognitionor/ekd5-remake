#!/bin/sh
# Cloudflare Pages 배포 (비밀번호 잠금 — 원본 데이터 포함이므로 공개 금지)
set -e
cd "$(dirname "$0")/.."
./scripts/build-web.sh
mkdir -p deploy/public
rsync -a --delete --copy-links --exclude data web/ deploy/public/
rsync -a --delete data/ deploy/public/data/
VER=$(date +%s)
echo "$VER" > deploy/public/version.txt
# 켤 때 확인용: 같은 번호를 글루에 새김 (web/ekd5.js 의 BUILD_VER = "dev")
sed -i '' "s/const BUILD_VER = \"dev\";/const BUILD_VER = \"$VER\";/" deploy/public/ekd5.js
grep -q "const BUILD_VER = \"$VER\";" deploy/public/ekd5.js
cd deploy && wrangler pages deploy --branch main --commit-dirty=true
