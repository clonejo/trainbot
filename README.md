# Onlytrains

**⚠️ Onlytrains is no longer being maintained due to time constraints. The frontend, with some archived data still available, is at https://trains.jo-m.ch. ⚠️**

Other deployments:

- <https://trains.shakik.de/>
- <https://trains.rahix.de/>

Watches a piece of train track, detects passing trains, and stitches together images of them.
Should work with any video4linux USB cam, or Raspberry Pi camera v3 modules.

The name Onlytrains is credited to [@timethy](https://github.com/timethy).

[<img src="internal/pkg/stitch/testdata/set0/day.jpg">](internal/pkg/stitch/testdata/set0/day.jpg)
[<img src="internal/pkg/stitch/testdata/set0/night.jpg">](internal/pkg/stitch/testdata/set0/night.jpg)
[<img src="internal/pkg/stitch/testdata/set0/rain.jpg">](internal/pkg/stitch/testdata/set0/rain.jpg)
[<img src="internal/pkg/stitch/testdata/set0/snow.jpg">](internal/pkg/stitch/testdata/set0/snow.jpg)
[<img src="demo.gif">](demo.gif)

It also contains some packages which might be useful for other purposes:

* [pkg/pmatch](pkg/pmatch/pkg.go): Image patch matching
* [pkg/ransac](pkg/ransac/ransac.go): RANSAC algorithm implementation
* [pkg/vk](pkg/vk/pkg.go): Vulkan compute framework

The binaries are currently built and tested on X86_64 and a Raspberry Pi 4 B.

## Assumptions and notes on computer vision

The computer vision used in trainbot is fairly naive and simple.
There is no camera calibration, image stabilization, undistortion, perspective mapping, or "real" object tracking.
This allows us to stay away from complex dependencies like OpenCV, and keeps the computational requirements low.
All processing happens on CPU.

The assumptions are (there might be more implicit ones):

1. Trains only appear in a (manually) pre-cropped region.
1. The camera is stable and the image does not move around in any direction.
1. There are no large fast brightness changes.
1. Trains have a given min and max speed (configurable).
1. We are looking at the tracks more or less perpendicularly in the chosen image crop region.
1. Trains are coming from one direction at a time, crossings are not handled properly
  1. In practice, they happen and lead to the result of one train being chopped up, e.g. https://trains.jo-m.ch/#/trains/19212.
1. Trains have a constant acceleration (might be 0) and do not stop and turn around while in front of the camera.
  1. In reality, this is often not true, there happens to be a stop signal right in front of my balcony...

## Documentation

As this is just a hobby project for me, the documentation is pretty sparse.
This very README is the most important part of it.
To deploy this project yourself, you should have some basic sysadmin, web servers, and ideally Go knowledge.
When in doubt, the source of truth is ... the source code.

All config options can be passed as ENV vars or CLI flags.
See `config struct` on top of `cmd/trainbot/main.go`, or run `trainbot --help` to see all options.

The two Makefiles (root and frontend/) also contain some hints.

## Deployment

There are two parts to deploy: First, the Go binary which detects trains, and second the web frontend.

How to get binaries?
There are multiple options:

1. `go install jo-m.ch/go/trainbot/cmd/trainbot@latest`
2. Grab a binary from the latest CI run at https://github.com/jo-m/trainbot/actions
3. Build via tooling in this repo - see [Development](#development)

### Raspberry Pi

Run the interactive tool to adjust camera and select a crop rectangle:

```bash
# On the host machine
make deploy_confighelper host=TRAINBOT_DEPLOY_TARGET_SSH_HOST
# Example:
make deploy_confighelper host=pi@10.20.0.12

# On the raspberry pi
sudo usermod -a -G video pi
# The --input arg has to be adapted to your actual camera config.
./confighelper-arm64 --log-pretty --input=picam3 --listen-addr=0.0.0.0:8080
```

Example "Production" deployment to a remote host (will install a systemd user unit):

First, you need to create a `env` file (copy `env.example`).
Then, from the host machine:

```bash
make deploy_trainbot host=TRAINBOT_DEPLOY_TARGET_SSH_HOST

# To see logs, on the target device:
journalctl --user -eu trainbot.service
```

Download latest data from Raspberry Pi:

```bash
ssh "$TRAINBOT_DEPLOY_TARGET_SSH_HOST" sqlite3 trainbot/data/db.sqlite3
.backup trainbot/data/db.sqlite3.bak
# Ctrl+D
rsync --verbose --archive --rsh=ssh "$TRAINBOT_DEPLOY_TARGET_SSH_HOST:trainbot/data/" data/
rm data/db.sqlite3-shm data/db.sqlite3-wal
mv data/db.sqlite3.bak data/db.sqlite3
```

### Frontend

## Web frontend

The frontend is a VueJS SPA app written in Typescript.
It consists of only static files (after the JS build process).
There is no web backend, the frontend simply loads the entire SQLite database from the server, and then does all the queries itself.
This means that the frontend can be deployed entirely independently from the trainbot binary, as long as there is some way for the date (db + pics) to get to the web server.

#### My setup

My Raspberry Pi is not exposed to the internet, and I also already had a web hosting account with FTP access available.
Thus, in my setup, the binary and the frontend are running on two entirely different machines in two different networks.

The frontend is built and deployed via:

```bash
export FRONTEND_DEPLOY_TARGET_SSH_HOST=myuser@mywebserver:/var/www/trains/
cd frontend
make deploy
```

The binary on the Raspberry Pi in my home network will upload pictures and the updated db file via FTP to this webspace whenever a new train is detected.
This is configured via the `ENABLE_UPLOAD=true` and `UPLOAD_...` env vars (or the corresponding CLI flags).

Alternative uploaders (e.g. SFTP, SCP, WebDAV, ...) could be pretty easily implemented (but they are not because I do not need them).
For this, the `Uploader` interface from `internal/pkg/upload/upload.go` needs to be implemented, and corresponding configuration options added.

#### Hosting the frontend on the same machine

It is possible to deploy the fronted on the same machine where trainbot runs.
There is no finished solution provided in this repo, but some hints are here:

- Install an arbitrary static web server (Nginx, Apache, Caddy, ...).
  - A webserver could also added to the trainbot binary itself, see e.g. [here](https://eli.thegreenplace.net/2022/serving-static-files-and-web-apps-in-go/), PRs welcome.
  - As wwwroot, this webserver needs the build output of the frontend, i.e. `cd frontend; make build; [s]cp dist /var/www/trains`.
- Set up the trainbot binary to have its data directory somewhere inside the wwwroot via `--data-dir / DATA_DIR`.
  - Assuming the wwwroot is `/var/www/trains`, trainbot would be running with `--data-dir=/var/www/trains/data`

Note that this can lead to transient inconsistencies when the web server is delivering the sqlite file at the same time the binary is writing to it.
The clean solution would be to add another "local FS" uploader to trainbot (see previous section).

### Hardware

I use a Raspberry Pi 4 Mod B with 2GiB of RAM, and a Raspberry Pi Camera v3 (narrow lens).
Distance from camera to tracks is ca. 50m.

All this is installed on my balcony in a waterproof case, as seen in the [MagPi Magazine](https://magpi.raspberrypi.com/issues/131).

The case is this one from AliExpress: https://www.aliexpress.com/item/1005003010275396.html

#### 3D Prints

- Mounting plate for Camera: https://www.tinkercad.com/things/1FowVwonymJ
- Mounting plate for Raspberry Pi: https://www.tinkercad.com/things/djlEF6oQSY1
- The prints were ordered from JLCPCB.

**Errata (not corrected in the models linked above):**

- The RPI mounting plate is 1-2mm too wide, because the 86mm stated in the picture on the Aliexpress product page are in reality a bit less.
  - You can solve that by changing the 3d design, or by cutting off a bit from the print. It might however also depend on your specific case.
- The RPI USB C power plug does not fit into the case, because the case is in the way.
  I solved this by cutting it off and soldering the cable to the connector perpendicularly.
  You can probably fix this by changing the 3D design to move the RPI to the left as far as possible.

## Development

This repo is set up to compile for `x86_64` and `aarch64`.
There is support for building on your machine directly, or inside a Docker container.

Also, there is an extensive test suite.
Tests may also be executed locally, or inside Docker.

The single entrypoint for everything (incl. Docker) is the `Makefile`.
You can list available targets via `make list`.
Same is true for the frontend - check out [frontend/Makefile](frontend/Makefile).

Example:

```bash
git clone https://github.com/jo-m/trainbot
cd trainbot
make docker_build

# Find binaries in build/ after this has completed.
```

## Vulkan

To work with the Vulkan code, you need to activate the `vk` build tag.
For VSCode, add the following to `.vscode/settings.json`:

```
"go.buildTags": "vk,moretests"
```

## V4L Settings

```bash
# list
ffmpeg -f v4l2 -list_formats all -i /dev/video2
v4l2-ctl --all --device /dev/video2

# exposure
v4l2-ctl -c exposure_auto=3 --device /dev/video2

# autofocus
v4l2-ctl -c focus_auto=1 --device /dev/video2

# fixed
v4l2-ctl -c focus_auto=0 --device /dev/video2
v4l2-ctl -c focus_absolute=0 --device /dev/video2
v4l2-ctl -c focus_absolute=1023 --device /dev/video2

ffplay -f video4linux2 -framerate 30 -video_size 3264x2448 -pixel_format mjpeg /dev/video2
ffplay -f video4linux2 -framerate 30 -video_size 1920x1080 -pixel_format mjpeg /dev/video2

ffmpeg -f v4l2 -framerate 30 -video_size 3264x2448 -pixel_format mjpeg -i /dev/video2 output.avi
```

## RasPi Cam v3 utils

```bash
# setup
sudo apt-get install libcamera0 libcamera-apps-lite
sudo apt install -y vlc

# grab frame
# https://www.raspberrypi.com/documentation/computers/camera_software.html#libcamera-and-libcamera-apps
libcamera-jpeg -o out.jpg -t 1 --width 4608 --height 2592 --rotation 180 --autofocus-mode=manual --lens-position=2
libcamera-jpeg -o out.jpg -t 1 --width 2304 --height 1296 --rotation 180 --autofocus-mode=manual --lens-position=4.5 --roi 0.25,0.5,0.5,0.5

# record video
DATE=$(date +'%F_%H-%M-%S'); libcamera-vid -o $DATE.h264 --save-pts $DATE.txt --width 1080 --height 720 --rotation 180 --autofocus-mode=manual --lens-position=0 -t 0

# stream through network
libcamera-vid -t 0 --inline --nopreview --width 4608 --height 2592 --rotation 180 --codec mjpeg --framerate 5 --listen -o tcp://0.0.0.0:8080 --autofocus-mode=manual --lens-position=0 --roi 0.25,0.5,0.5,0.5
# on localhost
ffplay http://pi4:8080/video.mjpeg

# manually record video for test cases
libcamera-vid \
   --verbose=1 \
   --timeout=0 \
   --inline \
   --nopreview \
   --width 240 --height 280 \
   --roi 0.429688,0.185185,0.104167,0.216049 \
   --mode=2304:1296:12:P \
   --framerate 30 \
   --autofocus-mode=manual --lens-position=0.000000 \
   --rotation=0 \
   -o vid.h264 --save-pts vid-timestamps.txt

mkvmerge -o test.mkv --timecodes 0:vid-timestamps.txt vid.h264
```

## How to create mask

- fetch a background image
  1. either from confighelper
      - select your rectangle in confighelper
      - configure rectangle in trainbot
      - take a screenshot, scale must be 1:1
      - open screenshot in GIMP
      - select rectangle (dimensions must be exact!)
      - Image -> Crop to Selection
  2. or from gif (video clip)
      - open train .gif in gimp
      - in the layers list, delete layers from the top until you find a good one
      - flatten image
- in layer list, right click layer, add mask
- select the mask thumbnail
- Using paintbrush tool, hide the obstructions (foreground) in black, white will be kept. Prefer to leave only a vertical pillar visible, to avoid artifacts.
- CTRL-A, CTRL-C
- create new file
- CTRL-V, merge / anchor floating layer down
- Colors -> Color to Alpha
- Color: black, OK
- File -> Export to mask.png
- configure `RECT_MASK=mask.png`, upload mask.png to where you run trainbot

If you end up with black artifacts, the pillar you let through is too small for the speed this train is travelling at.

## Code notes

* Zerolog is used as logging framework
* "Library" code uses `panic()`, "application" code use `log.Panic()...`

## Prometheus metrics/Grafana

For debugging and tweaking a [Prometheus](https://prometheus.io/)-compatible endpoint can be exposed at port 18963 using `--prometheus=true`. A [Grafana dashboard](grafana/Onlytrains-dashboard.json) is also available.

## Flow chart for frame data
```

           libcamera-vid
                 │
                 ▼
        ┌─────────────────┐
        │                 │
        │   source queue  │
        │                 │
        └─────────────────┘
                 │
                 ▼
             findOffset ──────► discard
                 │
              record
                 │
                 ▼
          ┌────────────┐
          │            │
          │  sequence  │
          │            │
          └────────────┘
                 │
                 ▼
               fitDx
                 │
                 ▼
              stitch
                 │
                 ▼
           ┌───────────┐
           │           │
           │   image   │
           │           │
           └───────────┘

```

## TODOs

- [ ] Replace deprecated `s.cam.GetOutput()`
- [ ] Fix false positives in darkness
- [ ] Add machine learning to classify trains (MobileNet, EfficientNet, https://mediapipe-studio.webapps.google.com/demo/image_classifier)
- [ ] Remote blob cleanup is broken due to FTP LIST being restricted to 99998 entries by remote - use sftp instead
- [ ] Select image processing methods depending on build tags (Vulkan)
- [ ] Maybe use some stuff from https://daniel.lawrence.lu/blog/y2025m09d21/
