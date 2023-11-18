package vid

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"image"
	"image/jpeg"
	"path/filepath"
	"sort"
	"time"

	"github.com/jo-m/trainbot/pkg/imutil"
	"github.com/vladimirvivien/go4vl/device"
	"github.com/vladimirvivien/go4vl/v4l2"
)

const (
	skipInitialFrames = 5
	bufferSize        = 5
)

// CamConfig describes an available camera device with a given pixel format and frame size.
type CamConfig struct {
	// For example /dev/video0.
	DeviceFile string

	// Format is the image format FourCC to request from the camera, for example "MJPG".
	// To list available formats and frame sizes:
	//
	//   v4l2-ctl --list-formats-ext --device /dev/video2
	Format FourCC `json:"-"`
	// FormatStr is Format converted to a string. It is only used for JSON serialization and does not need to be set
	// when opening a camera.
	FormatStr string `json:"Format"`
	FrameSize image.Point
}

func probeCam(deviceFile string) ([]CamConfig, error) {
	dev, err := device.Open(deviceFile)
	if err != nil {
		return nil, err
	}
	defer dev.Close()

	formats, err := dev.GetFormatDescriptions()
	if err != nil {
		return nil, err
	}

	ret := []CamConfig{}
	for _, format := range formats {
		sizes, err := v4l2.GetFormatFrameSizes(dev.Fd(), format.PixelFormat)
		if err != nil {
			return nil, err
		}

		for _, sz := range sizes {
			// Do not support variable sized frames.
			if sz.Size.MinWidth != sz.Size.MaxWidth || sz.Size.MinHeight != sz.Size.MaxHeight {
				continue
			}

			ret = append(ret, CamConfig{
				DeviceFile: deviceFile,
				Format:     FourCC(sz.PixelFormat),
				FormatStr:  FourCC(sz.PixelFormat).String(),
				FrameSize:  image.Pt(int(sz.Size.MaxWidth), int(sz.Size.MaxHeight)),
			})
		}
	}

	return ret, nil
}

// DetectCams returns a list of detected cameras and their supported pixel formats and frame sizes.
// This works even if some of the devices are currently in use.
// Cameras which list no available pixel formats, or produce errors on open, are ignored.
// Only fixed frame sizes are included.
func DetectCams() ([]CamConfig, error) {
	devices, err := filepath.Glob("/dev/video*")
	if err != nil {
		return nil, err
	}

	ret := []CamConfig{}
	for _, f := range devices {
		configs, err := probeCam(f)
		if err != nil {
			continue
		}
		ret = append(ret, configs...)
	}

	sort.Slice(ret, func(i, j int) bool {
		a, b := ret[i], ret[j]

		// Sort by device file name.
		if a.DeviceFile != b.DeviceFile {
			return a.DeviceFile < b.DeviceFile
		}

		// Prefer MJPEG.
		if a.Format != b.Format {
			return a.Format == FourCCMJPEG
		}

		return a.FrameSize.X*a.FrameSize.Y >= b.FrameSize.X*b.FrameSize.Y
	})

	return ret, nil
}

// CamSrc is a video frame source which supports video4linux.
// Use NewCamSrc to open one.
type CamSrc struct {
	c    CamConfig
	cam  *device.Device
	fmt  v4l2.PixFormat
	stop func()
	fps  uint32
}

// Compile time interface check.
var _ Src = (*CamSrc)(nil)

// NewCamSrc tries to open the specified frame source for frame reading.
func NewCamSrc(c CamConfig) (ret *CamSrc, err error) {
	fmt := v4l2.PixFormat{
		PixelFormat: v4l2.FourCCType(c.Format),
		Width:       uint32(c.FrameSize.X),
		Height:      uint32(c.FrameSize.Y),
	}
	cam, err := device.Open(
		c.DeviceFile,
		device.WithPixFormat(fmt),
		device.WithBufferSize(bufferSize),
	)
	if err != nil {
		return nil, err
	}

	pixFmt, err := cam.GetPixFormat()
	if err != nil {
		_ = cam.Close()
		return nil, err
	}

	if pixFmt.Width != uint32(c.FrameSize.X) || pixFmt.Height != uint32(c.FrameSize.Y) {
		_ = cam.Close()
		return nil, errors.New("image size does not match requested one")
	}

	ctx, stop := context.WithCancel(context.Background())
	if err := cam.Start(ctx); err != nil {
		_ = cam.Close()
		stop()
		return nil, err
	}

	fps, err := cam.GetFrameRate()
	if err != nil {
		_ = cam.Close()
		stop()
		return nil, err
	}

	ret = &CamSrc{
		c:    c,
		cam:  cam,
		fmt:  pixFmt,
		stop: stop,
		fps:  fps,
	}

	// We now skip some initial frames, because some cameras will return garbage in the first frame(s).
	for i := 0; i < skipInitialFrames; i++ {
		_, _, err := ret.getFrame()
		if err != nil {
			_ = cam.Close()
			return nil, err
		}
	}

	return ret, nil
}

// Close implements Src.
func (s *CamSrc) Close() error {
	s.stop()
	return s.cam.Close()
}

// IsLive implements Src.
func (s *CamSrc) IsLive() bool {
	return true
}

// GetFPS implements Src.
func (s *CamSrc) GetFPS() float64 {
	return float64(s.fps)
}

// getFrame retrieves a raw frame buffer from the camera.
func (s *CamSrc) getFrame() ([]byte, *time.Time, error) {
	frame := <-s.cam.GetOutput()
	ts := time.Now()
	return frame, &ts, nil
}

// convertFrame tries to decode a raw frame from the camera specified image format.
func (s *CamSrc) convertFrame(frame []byte) (image.Image, error) {
	switch s.c.Format {
	case FourCCMJPEG:
		// shamelessly stolen from:
		// https://github.com/filiptc/gorbit/blob/fa87ff39b68a6706306f34c318e0b9a5a3c97110/image/overlay.go#L37-L40
		parts := bytes.Split(frame, []byte{255, 218})
		b := bytes.NewBuffer(append(parts[0], append([]byte{255, 196}, append([]byte{1, 162, 0, 0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 1, 0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 16, 0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125, 1, 2, 3, 0, 4, 17, 5, 18, 33, 49, 65, 6, 19, 81, 97, 7, 34, 113, 20, 50, 129, 145, 161, 8, 35, 66, 177, 193, 21, 82, 209, 240, 36, 51, 98, 114, 130, 9, 10, 22, 23, 24, 25, 26, 37, 38, 39, 40, 41, 42, 52, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121, 122, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 17, 0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119, 0, 1, 2, 3, 17, 4, 5, 33, 49, 6, 18, 65, 81, 7, 97, 113, 19, 34, 50, 129, 8, 20, 66, 145, 161, 177, 193, 9, 35, 51, 82, 240, 21, 98, 114, 209, 10, 22, 36, 52, 225, 37, 241, 23, 24, 25, 26, 38, 39, 40, 41, 42, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121, 122, 130, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 226, 227, 228, 229, 230, 231, 232, 233, 234, 242, 243, 244, 245, 246, 247, 248, 249, 250}, append([]byte{255, 218}, parts[1]...)...)...)...))
		//err := os.WriteFile("destination.jpg", b.Bytes(), 0644)
		//if err != nil {
		//}
		return jpeg.Decode(b)
		//fails to decode my luckyzoom camera :( ("invalid JPEG format: uninitialized Huffman table")
		// the dumped image opens in eog and firefox flawlessly :(
		// cause and fix: https://github.com/blackjack/webcam/issues/7#issuecomment-231537816
	case FourCCYUYV:
		// YUYV: 4 bytes are 2 pixels.
		if len(frame) != s.c.FrameSize.X*s.c.FrameSize.Y*2 {
			return nil, errors.New("frame size does not match")
		}

		rect := image.Rectangle{image.Point{}, s.c.FrameSize}
		buf := make([]byte, len(frame))
		copy(buf, frame)
		return &imutil.YCbCr{
			Pix:  buf,
			Rect: rect,
		}, nil
	default:
		return nil, errors.New("unsupported format")
	}
}

// GetFrame implements Src.
func (s *CamSrc) GetFrame() (image.Image, *time.Time, error) {
	frame, ts, err := s.getFrame()
	if err != nil {
		return nil, nil, err
	}

	img, err := s.convertFrame(frame)
	if err != nil {
		return nil, nil, fmt.Errorf("unable to convert frame: %w", err)
	}

	return img, ts, nil
}

// GetFrameRaw returns a raw frame in the specified pixel format from the camera.
func (s *CamSrc) GetFrameRaw() ([]byte, FourCC, *time.Time, error) {
	frame, ts, err := s.getFrame()
	if err != nil {
		return nil, 0, nil, err
	}

	return frame, s.c.Format, ts, nil
}
