#!/usr/bin/env bash
# Creates private skein-fixture-* repos on the signed-in gh account. Re-running skips repos that exist.
# Remove them with: scripts/testing/github-fixtures.sh --delete   (needs: gh auth refresh -s delete_repo)
set -euo pipefail

owner=$(gh api user --jq .login)
repos=(skein-fixture-api skein-fixture-web skein-fixture-infra skein-fixture-mono)

if [[ ${1:-} == --delete ]]; then
  for r in "${repos[@]}"; do gh repo delete "$owner/$r" --yes || true; done
  exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
export GIT_AUTHOR_NAME=admin GIT_AUTHOR_EMAIL=admin@example.invalid
export GIT_COMMITTER_NAME=admin GIT_COMMITTER_EMAIL=admin@example.invalid
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1

n=0
commit() {
  n=$((n + 1))
  local date="2026-09-01T10:$(printf %02d $((n % 60))):00+03:00"
  git add -A
  GIT_AUTHOR_DATE=$date GIT_COMMITTER_DATE=$date git commit -q -m "$1"
}

init() {
  rm -rf "$work/$1" && mkdir -p "$work/$1" && cd "$work/$1"
  git init -q -b main
  git config credential.helper '!gh auth git-credential'
}

publish() {
  local name=$1
  if gh repo view "$owner/$name" >/dev/null 2>&1; then
    echo "skip $name (exists)"
    return
  fi
  gh repo create "$owner/$name" --private --description "Skein test fixture; safe to delete" >/dev/null
  git remote add origin "https://github.com/$owner/$name.git"
  git push -q --all origin
  git push -q --tags origin
  echo "created $name"
}

build_api() {
  init skein-fixture-api
  mkdir -p src/routes docs
  printf 'export const port = 8080;\n' > src/config.ts
  printf 'line one\r\nline two\r\n' > docs/windows-crlf.txt
  printf '\xef\xbb\xbfBOM first line\nsecond\n' > docs/bom.txt
  printf '\x89PNG\r\n\x1a\n\x00\x00binary' > docs/logo.png
  for r in users orders billing; do printf 'export function %s() { return []; }\n' "$r" > "src/routes/$r.ts"; done
  commit "Initial API"
  git tag v1.0
  printf 'export const port = 8081;\nexport const debug = false;\n' > src/config.ts
  commit "Bump port"
  git checkout -q -b release/2.4
  printf 'export function billing() { return ["invoice"]; }\n' > src/routes/billing.ts
  commit "Release billing fix"
  printf '# Release 2.4\n' > docs/RELEASE.md
  commit "Release notes"
  git checkout -q main
  printf 'export function orders() { return ["new"]; }\n' > src/routes/orders.ts
  commit "Orders on main"
  git checkout -q -b feature/login
  printf 'export function login() { return true; }\n' > src/routes/login.ts
  git rm -q src/routes/users.ts
  commit "Login replaces users"
  git checkout -q main
  publish skein-fixture-api
}

build_web() {
  init skein-fixture-web
  for d in components pages lib styles; do
    mkdir -p "src/$d"
    for i in $(seq 1 75); do printf '// %s %d\nexport const v%d = %d;\n' "$d" "$i" "$i" "$i" > "src/$d/f$i.ts"; done
  done
  commit "Scaffold web client"
  git checkout -q -b develop
  for i in $(seq 1 20); do printf '// changed on develop\nexport const v%d = %d;\n' "$i" $((i * 10)) > "src/components/f$i.ts"; done
  git mv src/lib/f1.ts src/lib/renamed.ts
  git rm -q src/styles/f75.ts
  mkdir -p src/new && printf 'export {};\n' > src/new/only-develop.ts
  commit "Develop changes"
  git checkout -q main
  printf '// hotfix\nexport const v2 = 2;\n' > src/pages/f2.ts
  commit "Hotfix on main"
  publish skein-fixture-web
}

build_infra() {
  init skein-fixture-infra
  mkdir -p terraform
  printf 'variable "region" { default = "eu-central-1" }\n' > terraform/main.tf
  head -c 1048576 /dev/zero | tr '\0' 'x' > terraform/large.txt
  commit "Infra baseline"
  publish skein-fixture-infra
}

build_mono() {
  init skein-fixture-mono
  for p in $(seq 1 30); do
    mkdir -p "packages/p$p/src"
    for i in $(seq 1 50); do printf 'export const p%d_%d = %d;\n' "$p" "$i" "$i" > "packages/p$p/src/m$i.ts"; done
  done
  commit "Monorepo 1500 files"
  git checkout -q -b perf/a
  for p in $(seq 1 30 | awk 'NR % 3 == 0'); do
    for i in $(seq 1 10); do printf 'export const p%d_%d = %d; // perf\n' "$p" "$i" $((i + 1)) >> "packages/p$p/src/m$i.ts"; done
  done
  commit "Perf branch edits"
  git checkout -q main
  publish skein-fixture-mono
}

build_api
build_web
build_infra
build_mono
