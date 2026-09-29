#!/usr/bin/env bash
# Checks PolarEffects' GRIB output with ecCodes, an independent decoder
# (spec.md 7.8, invariant 5). Run by CI; locally when ecCodes is installed
# (`brew install eccodes`, `apt-get install libeccodes-tools`).
#
#   tools/check-grib.sh sample.grib2 [golden.grib2 ...]
#
# The first file is the output of `cargo run -p pe-grib --example emit`:
# 24 messages over two regional grids (the Channel across the prime
# meridian, the Pacific across the antimeridian), each field analytic
# (see crates/pe-grib/examples/emit.rs). ecCodes must parse every message,
# report the grid corners and times written, and decode every value to the
# formula within the 16-bit packing step. Every further file (the golden
# exports) must parse and dump without error.
set -euo pipefail

sample=${1:?usage: check-grib.sh sample.grib2 [more.grib2 ...]}
shift

grib_ls "$sample" > /dev/null
grib_dump "$sample" > /dev/null
test "$(grib_count "$sample")" = "24"

# Grid corners and times, as ecCodes reads them.
keys=Ni,Nj,latitudeOfFirstGridPointInDegrees,longitudeOfFirstGridPointInDegrees,latitudeOfLastGridPointInDegrees,longitudeOfLastGridPointInDegrees,dataDate,dataTime,step,shortName,discipline,parameterCategory,parameterNumber
expected_grid() {
  if [ "$1" -le 12 ]; then echo "50 22 53 348.5 47.75 0.75"; else echo "97 21 23 168 18 192"; fi
}
for n in $(seq 1 24); do
  got=$(grib_get -w count="$n" -p "$keys" "$sample")
  read -r ni nj la1 lo1 la2 lo2 date time step short disc cat num <<< "$got"
  want=$(expected_grid "$n")
  test "$ni $nj $la1 $lo1 $la2 $lo2" = "$want" || { echo "message $n: grid $ni $nj $la1 $lo1 $la2 $lo2, expected $want" >&2; exit 1; }
  test "$date $time" = "20200727 1200" || { echo "message $n: reference $date $time" >&2; exit 1; }
  test "$step" = "$(( ((n - 1) / 6) % 2 ))" || { echo "message $n: step $step" >&2; exit 1; }
  params=("0 2 2" "0 2 3" "10 0 3" "10 0 14" "10 1 2" "10 1 3")
  test "$disc $cat $num" = "${params[$(( (n - 1) % 6 ))]}" || { echo "message $n: parameter $disc $cat $num" >&2; exit 1; }
done

# Every value against its formula (emit.rs `field`); longitudes as ecCodes
# prints them are folded into [-180, 180) first.
for n in $(seq 1 24); do
  grib_get_data -w count="$n" -m MISSING "$sample" | awk -v n="$n" '
    NR == 1 { next }
    {
      lat = $1; lon = $2; value = $3
      while (lon >= 180) lon -= 360
      while (lon < -180) lon += 360
      p = (n - 1) % 6; hour = int((n - 1) / 6) % 2
      mid = (n <= 12) ? (53 + 47.75) / 2 : (23 + 18) / 2
      if (p == 0) want = lat / 10
      else if (p == 1) want = lon / 10
      else if (p == 2) want = (lat > mid) ? "MISSING" : lat / 10
      else if (p == 3) want = lon + 180
      else if (p == 4) want = hour / 100
      else want = -lat / 100
      if (want == "MISSING" || value == "MISSING") {
        if (want != value) { printf "message %d at %s %s: %s, expected %s\n", n, $1, $2, value, want; bad = 1; exit 1 }
        next
      }
      d = value - want; if (d < 0) d = -d
      if (d > 0.01) { printf "message %d at %s %s: %s, expected %s\n", n, $1, $2, value, want; bad = 1; exit 1 }
      count++
    }
    END { if (!bad && count == 0) { print "message " n ": no values"; exit 1 } }'
done

for file in "$@"; do
  grib_ls "$file" > /dev/null
  grib_dump "$file" > /dev/null
  echo "$file: $(grib_count "$file") messages read by ecCodes"
done
echo "ecCodes read $sample: 24 messages, grids, times and values as written"
