#!/usr/bin/env bash
# Monta el motor autocontenido de Heliora en una carpeta: un Python portable
# (python-build-standalone) con las librerías de engine/requirements.txt y el código de engine/.
# La aplicación lo encuentra en <destino>/engine y <destino>/python (ver src/motor.rs).
#
# Uso: packaging/preparar_motor.sh <destino> <plataforma>
#   plataforma: x86_64-pc-windows-msvc | aarch64-apple-darwin | x86_64-apple-darwin
set -euo pipefail

DESTINO="$1"
PLATAFORMA="$2"
# Versión fijada: el mismo instalador se reconstruye igual en cada ejecución
PBS_FECHA="20241016"
PBS_PYTHON="3.12.7"
URL="https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_FECHA}/cpython-${PBS_PYTHON}+${PBS_FECHA}-${PLATAFORMA}-install_only.tar.gz"
RAIZ="$(cd "$(dirname "$0")/.." && pwd)"

mkdir -p "$DESTINO"
rm -rf "$DESTINO/python" "$DESTINO/engine"
echo "Descargando Python ${PBS_PYTHON} (${PLATAFORMA})"
curl -fsSL "$URL" | tar -xz -C "$DESTINO"

case "$PLATAFORMA" in
  *windows*) PY="$DESTINO/python/python.exe" ;;
  *) PY="$DESTINO/python/bin/python3" ;;
esac

"$PY" -m pip install --no-cache-dir --disable-pip-version-check --no-warn-script-location -q -r "$RAIZ/engine/requirements.txt"

# Copia del motor sin cachés ni salidas de ejecución
mkdir -p "$DESTINO/engine"
( cd "$RAIZ/engine" && tar --exclude='__pycache__' --exclude='*.pyc' -cf - . ) | tar -xf - -C "$DESTINO/engine"
"$PY" -m compileall -q "$DESTINO/engine" > /dev/null

# Comprobación: el motor arranca con el Python incluido y todas sus librerías
( cd "$DESTINO/engine" && "$PY" -c "import numpy, scipy, pandas, sklearn, statsmodels, pvlib, pyarrow, joblib; import heliora.servidor; print('Motor incluido: Python', __import__('sys').version.split()[0])" )
