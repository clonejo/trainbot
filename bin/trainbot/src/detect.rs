use std::time::{Duration, SystemTime};

use anyhow::Context;
use chrono::{DateTime, Local, Utc};
use clap::Parser;
use image::DynamicImage;
use rusqlite::Connection;
use tracing::{info, warn};

use store::{DataStore, queries};
use trainbot_core::{
    AutoStitcher, AutoStitcherError, Config, FitMethod, Train, VIDEO_EXTENSION, init_logging,
    init_metrics,
};
use vid::{FourCC, FrameSource};

use crate::args::DetectArgs;

const MAX_FAILED_FRAMES: usize = 50;
const MAX_JPEG_DIM: u32 = 32767;
const RECT_SIZE_MIN: u32 = 100;
const RECT_SIZE_MAX_WARN: u32 = 500;

pub fn run(argv: Vec<String>) {
    let args = DetectArgs::parse_from(argv);

    init_logging(&trainbot_core::LogConfig {
        log_pretty: args.log_pretty,
        log_level: args.log_level.clone(),
    });

    if args.rect_w == 0 || args.rect_h == 0 {
        eprintln!("error: no rect set (use --rect-.. parameters to set crop region)");
        std::process::exit(2);
    }
    if args.rect_w < RECT_SIZE_MIN || args.rect_h < RECT_SIZE_MIN {
        eprintln!("error: rect too small (minimum {} px)", RECT_SIZE_MIN);
        std::process::exit(2);
    }
    if args.rect_w > RECT_SIZE_MAX_WARN || args.rect_h > RECT_SIZE_MAX_WARN {
        warn!(
            "rect is very wide (over {} px). Live processing may not keep up.",
            RECT_SIZE_MAX_WARN
        );
    }

    if args.prometheus {
        init_metrics(&args.prometheus_listen);
    }

    let ds = DataStore::new(&args.data_dir);
    ds.create_dirs().expect("create blobs/failed dirs");

    let conn = store::open(ds.db_path()).expect("open DB");

    let mask = args.mask.as_ref().map(|path| {
        imutil::load(path)
            .unwrap_or_else(|e| panic!("load mask {path}: {e}"))
            .into_rgba8()
    });

    let fourcc = FourCC::parse(&args.camera_format_fourcc).unwrap_or_else(|| {
        eprintln!("error: invalid FourCC {:?}", args.camera_format_fourcc);
        std::process::exit(2);
    });

    let mut src = open_source(&args, fourcc).unwrap_or_else(|e| {
        eprintln!("error: failed to open video source: {e:#}");
        std::process::exit(1);
    });

    let is_picam3 = args.input == "picam3";
    let crop = (!is_picam3).then_some((args.rect_x, args.rect_y, args.rect_w, args.rect_h));

    // TODO: convert to enum
    let fit_method = match args.fit_method.as_str() {
        "ols" => FitMethod::Ols,
        "ransac" => FitMethod::Ransac,
        other => {
            eprintln!("error: unknown --fit-method {other:?} (expected ols|ransac)");
            std::process::exit(2);
        }
    };

    let config = Config {
        pixels_per_m: args.px_per_m,
        min_speed_kph: args.min_speed_kph,
        max_speed_kph: args.max_speed_kph,
        min_length_m: args.min_len_m,
        max_frame_count_per_seq: args.max_frame_count_per_seq,
        mask,
        video_encoder: args.video_encoder,
    };

    tracing::info!(input = %args.input, data_dir = %args.data_dir, "starting");

    let mut stitcher = AutoStitcher::new(config, fit_method);
    let mut failed_frames: usize = 0;

    let mut next_frame_ts = SystemTime::now();
    loop {
        match src.next_frame() {
            Ok(Some(mut frame)) => {
                if let Some(dt) = args.constant_frame_time_micros {
                    frame.ts = next_frame_ts;
                    next_frame_ts += Duration::from_micros(dt);
                }
                failed_frames = 0;
                let img = if let Some((x, y, w, h)) = crop {
                    image::imageops::crop_imm(&frame.image, x, y, w, h).to_image()
                } else {
                    frame.image
                };
                if let Ok(Some(train)) = stitcher.frame(img, frame.ts).inspect_err(|err| {
                    warn!(%err, "Failed to fit and stitch");
                    save_failed_video(err, &ds);
                }) && let Err(e) = save_train(&train, &ds, &conn)
                {
                    tracing::error!(err = %e, "failed to save train");
                }
            }
            Ok(None) => break,
            Err(e) => {
                failed_frames += 1;
                tracing::warn!(err = %e, failed_frames, "frame error");
                if !src.is_live() || failed_frames >= MAX_FAILED_FRAMES {
                    break;
                }
            }
        }
    }

    if let Ok(Some(train)) = stitcher.try_stitch_and_reset().inspect_err(|err| {
        warn!(%err, "Failed to fit and stitch");
        save_failed_video(err, &ds);
    }) && let Err(e) = save_train(&train, &ds, &conn)
    {
        tracing::error!(err = %e, "failed to save final train");
    }
}

fn open_source(args: &DetectArgs, fourcc: FourCC) -> anyhow::Result<Box<dyn FrameSource>> {
    if args.input.starts_with("http://") || args.input.starts_with("https://") {
        let src = vid::MjpegHttpSrc::open(&args.input).context("open MjpegHttpSrc")?;
        return Ok(Box::new(vid::BufSrc::new(src, args.src_buf_cap)));
    }

    if args.input == "picam3" {
        let src = vid::PiCam3Src::open(vid::PiCam3Config {
            roi_x: args.rect_x,
            roi_y: args.rect_y,
            width: args.rect_w,
            height: args.rect_h,
            focus: 0.0,
            rotate_180: args.rotate_180,
            format: fourcc,
            fps: 40, // TODO: make configurable
        })
        .context("open PiCam3")?;
        return Ok(Box::new(vid::BufSrc::new(src, args.src_buf_cap)));
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::FileTypeExt;
        if let Ok(meta) = std::fs::metadata(&args.input)
            && meta.file_type().is_char_device()
        {
            let src = vid::CamSrc::open(vid::CamConfig {
                device: args.input.clone(),
                fourcc,
                width: args.camera_w,
                height: args.camera_h,
            })
            .context("open CamSrc")?;
            return Ok(Box::new(vid::BufSrc::new(src, args.src_buf_cap)));
        }
    }

    Ok(Box::new(
        vid::FileSrc::open(&args.input).context("open FileSrc")?,
    ))
}

fn save_train(train: &Train, ds: &DataStore, conn: &Connection) -> anyhow::Result<()> {
    let dt_utc: DateTime<Utc> = train.start_ts.into();
    let dt_fixed = dt_utc.fixed_offset();

    // Use start_ts to generate filenames (id=0 is a placeholder before DB insert)
    let row = queries::Train {
        id: 0,
        start_ts: dt_fixed,
    };
    let img_name = row.img_file_name();
    let gif_name = row.gif_file_name();
    let video_name = row.file_name(VIDEO_EXTENSION);

    let img_path = ds.blob_path(&img_name);
    let thumb_path = ds.blob_thumb_path(&img_name);
    let gif_path = ds.blob_path(&gif_name);
    let video_path = ds.blob_path(&video_name);

    // Resize to JPEG-safe dimensions
    let dyn_img = DynamicImage::ImageRgba8(train.image.clone());
    let dyn_img = if dyn_img.width() > MAX_JPEG_DIM || dyn_img.height() > MAX_JPEG_DIM {
        dyn_img.thumbnail(MAX_JPEG_DIM, MAX_JPEG_DIM)
    } else {
        dyn_img
    };

    imutil::save_jpeg(&img_path, &dyn_img, 85).with_context(|| format!("save jpeg {img_name}"))?;

    let thumb = dyn_img.thumbnail(MAX_JPEG_DIM, 64);
    imutil::save_jpeg(&thumb_path, &thumb, 75).with_context(|| format!("save thumb {img_name}"))?;

    std::fs::write(&gif_path, &train.gif_data).with_context(|| format!("save gif {gif_name}"))?;
    std::fs::write(&video_path, &train.video_data)
        .with_context(|| format!("save video {video_name}"))?;

    let id = queries::insert_train(
        conn,
        &dt_fixed,
        train.n_frames as i64,
        train.length_px,
        train.speed_px_s,
        train.accel_px_s2,
        train.conf.pixels_per_m,
    )
    .context("insert_train")?;

    tracing::info!(
        id,
        speed_kph = train.speed_m_ps() * 3.6,
        accel_m_ps2 = train.accel_m_ps2(),
        n_frames = train.n_frames,
        img = %img_name,
        "train saved"
    );

    Ok(())
}

fn save_failed_video(err: &AutoStitcherError, ds: &DataStore) {
    let Some(video_data) = err.video_data() else {
        return;
    };
    let now = Local::now();
    let path = ds.failed_path(now.into(), VIDEO_EXTENSION);
    match std::fs::write(&path, video_data) {
        Ok(_) => info!(%path, "Stored stitching failure video."),
        Err(err) => warn!(%err, %path, "Could not write stitching failure video."),
    }
}
