#!/bin/sh
# Fetch GeneralUser GS 2.0.3 (S. Christian Collins), the SoundFont the score
# editor plays MIDI with, and cut it into parts the asset store takes (an
# asset is at most 16 MiB, EUI spec 10; the client puts them back together,
# 03 §7). 32 MB, so it is not in git: public/soundfonts/ is ignored.
#
# Its licence (public/soundfonts/LICENSE.txt) allows use in software; the
# author says he cannot be certain where every sample came from. Without
# the bank the editor still plays, on the client's own voices.
set -eu
cd "$(dirname "$0")/.."
REV=684543d5e5efaef08d02be50dcda8d552478fa60
SHA=9575028c7a1f589f5770fccc8cff2734566af40cd26ed836944e9a5152688cfe
BASE=https://github.com/mrbumpy409/GeneralUser-GS/raw/$REV
DIR=public/soundfonts
mkdir -p "$DIR"
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
curl -fsSL "$BASE/GeneralUser-GS.sf2" -o "$tmp"
echo "$SHA  $tmp" | sha256sum -c --quiet
curl -fsSL "$BASE/documentation/LICENSE.txt" -o "$DIR/LICENSE.txt"
rm -f "$DIR"/GeneralUser-GS.sf2.*
split -b 15M -d -a 1 "$tmp" "$DIR/GeneralUser-GS.sf2."
ls -l "$DIR"
