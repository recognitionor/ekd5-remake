#!/bin/sh
# Cloudflare Pages 배포 (비밀번호 잠금 — 원본 데이터 포함이므로 공개 금지)
set -e
cd "$(dirname "$0")/.."
./scripts/build-web.sh
mkdir -p deploy/public
rsync -a --delete --copy-links --exclude data web/ deploy/public/
rsync -a --delete data/ deploy/public/data/
date +%s > deploy/public/version.txt
cd deploy && wrangler pages deploy --branch main --commit-dirty=true
