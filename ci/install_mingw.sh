#!/bin/sh
set -ex
curl -sSf -O "${MINGW_URL}/${MINGW_ARCHIVE}"
7z x -y "${MINGW_ARCHIVE}" -o/c/mingw
rm "${MINGW_ARCHIVE}"
