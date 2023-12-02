package vid

import (
	"image"
	"image/color"
	"io"
	"time"

	"github.com/disintegration/imaging"
	"github.com/rs/zerolog/log"
)

const queueSize = 200

type frameWithTS struct {
	frame image.Image
	ts    time.Time
}

// SrcBuf buffers a video source.
// Use NewSrcBuf to create an instance.
type SrcBuf struct {
	src             Src
	maxFailedFrames int
	rotateAngle     float64
	queue           chan frameWithTS
	err             chan error
}

// Compile time interface check.
var _ Src = (*SrcBuf)(nil)

// NewSrcBuf creates a new SrcBuf.
// Will not close src, caller needs to do that after last frame is read.
func NewSrcBuf(src Src, maxFailedFrames int, rotateAngle float64) *SrcBuf {
	ret := SrcBuf{
		src:             src,
		maxFailedFrames: maxFailedFrames,
		rotateAngle:     rotateAngle,
		queue:           make(chan frameWithTS, queueSize),
		err:             make(chan error),
	}

	go ret.run()

	return &ret
}

func (s *SrcBuf) cleanup(err error) {
	close(s.queue)
	s.err <- err
	close(s.err)
}

func (s *SrcBuf) run() {
	live := s.src.IsLive()
	failedFrames := 0

	for {
		frame, ts, err := s.src.GetFrame()
		if err != nil {
			failedFrames++
			log.Warn().Err(err).Int("failedFrames", failedFrames).Msg("failed to retrieve frame")

			if err == io.EOF {
				s.cleanup(err)
				return
			}

			if failedFrames >= s.maxFailedFrames {
				log.Error().Msg("retrieving frames failed too many times, exiting")
				s.cleanup(err)
				return
			}

			continue
		}

		failedFrames = 0

		// Create copy, and rotate if needed.
		// angles of 0, 90, 180 and 270 are special-cased in imaging.Rotate()
		// For angle=0 Clone() is called.
		frame = imaging.Rotate(frame, s.rotateAngle, color.Black)

		if live {
			select {
			case s.queue <- frameWithTS{frame, *ts}:
			default:
				log.Warn().Msg("dropped frame")
			}
		} else {
			s.queue <- frameWithTS{frame, *ts}
		}
	}
}

// GetFrame returns the next frame.
// As soon as this returns an error once, the instance needs to be discarded.
// The underlying image buffer will be owned by the caller, src will not reuse or modify it.
func (s *SrcBuf) GetFrame() (image.Image, *time.Time, error) {
	f, ok := <-s.queue
	if ok {
		return f.frame, &f.ts, nil
	}

	return nil, nil, <-s.err
}

// GetFPS implements Src.
func (s *SrcBuf) GetFPS() float64 {
	return s.src.GetFPS()
}

// IsLive implements Src.
func (s *SrcBuf) IsLive() bool {
	return s.src.IsLive()
}

// Close implements Src.
func (s *SrcBuf) Close() error {
	panic("do not call this, instead close the underlying source yourself")
}

// GetFrameRaw implements Src.
func (s *SrcBuf) GetFrameRaw() ([]byte, FourCC, *time.Time, error) {
	panic("not implemented")
}
