#!/usr/bin/env bash
# Euler APT repo — setup aptly + GPG + publish
# Uso: ./tools/euler/repo/setup-aptly.sh
# Requiere: aptly, gnupg

set -euo pipefail

REPO_NAME="euler"
DIST="testing"
COMPONENT="main"
ARCH="amd64"
GPG_KEY="euler@euler.bo"
MIRROR_URL="http://deb.debian.org/debian"

need_cmd() { command -v "$1" >/dev/null || { echo "falta $1" >&2; exit 1; }; }
need_cmd aptly

# GPG
if ! gpg --list-secret-keys "$GPG_KEY" >/dev/null 2>&1; then
    echo "[gpg] Generando clave $GPG_KEY (RSA 4096, sin expiración)"
    gpg --batch --generate-key <<EOF
%no-protection
Key-Type: RSA
Key-Length: 4096
Subkey-Type: RSA
Subkey-Length: 4096
Name-Real: Euler OS
Name-Email: $GPG_KEY
Expire-Date: 0
EOF
fi

gpg --armor --export "$GPG_KEY" > /tmp/euler.gpg
echo "[gpg] Clave pública en /tmp/euler.gpg — distribuir a /etc/apt/trusted.gpg.d/euler.gpg en ISO"

# Aptly repo
if ! aptly repo show "$REPO_NAME" >/dev/null 2>&1; then
    aptly repo create -distribution="$DIST" -component="$COMPONENT" "$REPO_NAME"
    echo "[aptly] repo $REPO_NAME creado"
fi

# Mirror Debian testing (opcional, para snapshot reproducible)
if ! aptly mirror show debian-testing >/dev/null 2>&1; then
    aptly mirror create -architectures="$ARCH" debian-testing "$MIRROR_URL" "$DIST" "$COMPONENT"
    echo "[aptly] mirror debian-testing creado — correr 'aptly mirror update debian-testing' para sync"
fi

# Publicar
# aptly publish repo -gpg-key="$GPG_KEY" -distribution="$DIST" "$REPO_NAME" filesystem:euler:/
# O S3: aptly publish repo -gpg-key="$GPG_KEY" -distribution="$DIST" "$REPO_NAME" s3:euler:/
echo "[done] Para publicar: aptly publish repo -gpg-key=\"$GPG_KEY\" -distribution=\"$DIST\" $REPO_NAME"
echo "  Añadir .deb: aptly repo add $REPO_NAME build/*.deb && aptly publish update -gpg-key=\"$GPG_KEY\" $DIST"
