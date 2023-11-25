package stitch

import (
	"image"
	"testing"
)

func Test_AutoStitcher_Set3(t *testing.T) {
	c := Config{
		PixelsPerM:  140,
		MinSpeedKPH: 10,
		MaxSpeedKPH: 80,
		MinLengthM:  10,
	}
	//r := image.Rect(852, 34, 334, 837)
	//852, 34
	//334, 837

	//x": 843, "y": 23, "w": 289, "h": 910
	r := image.Rect(0, 0, 289, 910).Add(image.Pt(843, 23))

	runTestDetailed(t, c, r, "testdata/set3/ICE.mkv", "testdata/set3/ICE.jpg", 86, 21.53, 0.27, false)
}
