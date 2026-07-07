#!/usr/bin/env bash
set -euo pipefail

data=$1
if [[ ! -d $data ]]; then
	echo >&2 "$data is not a directory."
fi

cd "$data/blobs"

gifs=($(find . -name '*.gif' | sort | tac))

for old in "${gifs[@]}"; do
	new=${old%.gif}.mp4

	if [[ -e "$new" ]]; then
		echo >&2 "skipping $old"
		continue
	fi

	echo >&2 "$old -> $new"

	temp=$(mktemp --tmpdir=.. --suffix=".migrate-gif-mp4.mp4")

	# settings taken from trainbot/crates/core/src/video.rs
	# crf15 would be nice for night scenes, but crf20 is completely sufficient for day scenes. picking crf17 to save some space.
	# libx264 requires dimensions divisible by 2, so we pad the image if necessary. (https://stackoverflow.com/a/20848224/895727)
	ffmpeg \
		-loglevel repeat+level+warning \
		-i "$old" \
		-f mp4 \
		-fps_mode passthrough \
		-c:v libx264 \
		-b:v 2048k \
		-profile:v high \
		-vf "pad=ceil(iw/2)*2:ceil(ih/2)*2,format=yuv420p" \
		-preset slower \
		-crf 17 \
		-movflags +faststart \
		-y \
		"$temp"

	mv --interactive "$temp" "$new"

done
