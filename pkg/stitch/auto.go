package stitch

import (
	"image"
	"math"
	"time"

	"github.com/jo-m/trainbot/internal/pkg/imutil"
	"github.com/jo-m/trainbot/pkg/pmatch"
	"github.com/rs/zerolog/log"
)

const (
	goodScoreNoMove = 0.99
	goodScoreMove   = 0.95

	maxSeqLen = 800
)

type Config struct {
	PixelsPerM  float64
	MinSpeedKPH float64
	MaxSpeedKPH float64

	VideoFPS float64
}

func (e *Config) MinPxPerFrame() int {
	return int(e.MinSpeedKPH/3.6*e.PixelsPerM/e.VideoFPS) - 1
}

func (e *Config) MaxPxPerFrame() int {
	return int(e.MaxSpeedKPH/3.6*e.PixelsPerM/e.VideoFPS) + 1
}

type AutoStitcher struct {
	c            Config
	minDx, maxDx int

	prevCount      int
	prevFrameColor image.Image
	prevFrameGray  *image.Gray

	seq          sequence
	dxAbsLowPass float64
}

func NewAutoStitcher(c Config) *AutoStitcher {
	return &AutoStitcher{
		c:     c,
		minDx: c.MinPxPerFrame(),
		maxDx: c.MaxPxPerFrame(),
	}
}

func findOffset(prev, curr *image.Gray, maxDx int) (dx int, score float64) {
	if prev.Rect.Size() != curr.Rect.Size() {
		log.Panic().Msg("inconsistent size, this should not happen")
	}

	// centered crop from prev frame,
	// width is 3x max pixels per frame given by max velocity
	w := maxDx * 3
	// and 3/4 of frame height
	h := int(float64(prev.Rect.Dy())*3/4 + 1)
	subRect := image.Rect(0, 0, w, h).
		Add(curr.Rect.Min).
		Add(
			curr.Rect.Size().
				Sub(image.Pt(int(w), h)).
				Div(2),
		)
	sub, err := imutil.Sub(prev, subRect)
	if err != nil {
		log.Panic().Err(err).Msg("this should not happen")
	}

	// centered slice crop from next frame,
	// width is 1x max pixels per frame given by max velocity
	// and 3/4 of frame height
	w = maxDx
	sliceRect := image.Rect(0, 0, w, h).
		Add(curr.Rect.Min).
		Add(
			curr.Rect.Size().
				Sub(image.Pt(w, h)).
				Div(2),
		)

	slice, err := imutil.Sub(curr, sliceRect)
	if err != nil {
		log.Panic().Err(err).Msg("this should not happen")
	}

	// we expect this x value found by search
	// if nothing has moved
	xZero := sliceRect.Min.Sub(subRect.Min).X

	x, _, score := pmatch.SearchGrayC(sub.(*image.Gray), slice.(*image.Gray))
	return x - xZero, score
}

func (r *AutoStitcher) reset() {
	r.seq = sequence{}
	r.dxAbsLowPass = 0
}

func (r *AutoStitcher) record(dx int, ts time.Time, frame image.Image) {
	r.seq.dx = append(r.seq.dx, dx)
	r.seq.ts = append(r.seq.ts, ts)
	r.seq.frames = append(r.seq.frames, frame)
}

func iabs(i int) int {
	if i < 0 {
		return -i
	}
	return i
}

// Finalize tries to stitch any remaining frames and resets the instance.
func (r *AutoStitcher) Finalize() {
	if len(r.seq.dx) == 0 {
		log.Info().Msg("nothing to assemble")
		return
	}

	log.Info().Msg("end of sequence")
	err := fitAndStitch(r.seq)
	if err != nil {
		log.Err(err).Msg("unable to fit and stitch sequence")
	}
	r.reset()
}

// will make a copy of the image
func (r *AutoStitcher) Frame(frameColor image.Image, ts time.Time) {
	frameColor = imutil.ToRGBA(frameColor)
	frameGray := imutil.ToGray(frameColor)
	defer func() {
		r.prevFrameColor = frameColor
		r.prevFrameGray = frameGray
		r.prevCount++
	}()

	if r.prevFrameColor == nil {
		// first time
		return
	}

	dx, score := findOffset(r.prevFrameGray, frameGray, r.maxDx)
	log.Debug().Int("prevCount", r.prevCount).Int("dx", dx).Float64("score", score).Msg("received frame")

	isActive := len(r.seq.dx) > 0
	if isActive {
		r.dxAbsLowPass = r.dxAbsLowPass*0.9 + math.Abs(float64(dx))*0.1

		// Bail out before we use too much memory.
		if len(r.seq.dx) > maxSeqLen {
			r.Finalize()
			return
		}

		if r.dxAbsLowPass < r.c.MinSpeedKPH {
			r.Finalize()
			return
		}

		r.record(dx, ts, r.prevFrameColor)
		return
	} else {
		if score >= goodScoreNoMove && iabs(dx) < r.minDx {
			log.Debug().Msg("not moving")
			return
		}

		if score >= goodScoreMove && iabs(dx) >= r.minDx && iabs(dx) <= r.maxDx {
			log.Info().Msg("start of new sequence")
			r.record(dx, ts, r.prevFrameColor)
			r.dxAbsLowPass = math.Abs(float64(dx))
			return
		}
	}

	log.Debug().Float64("score", score).Int("dx", dx).Msg("inconclusive frame")
}