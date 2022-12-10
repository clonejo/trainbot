package pmatch

import (
	"image"
	_ "image/jpeg"
	_ "image/png"
	"testing"

	"github.com/jo-m/trainbot/internal/pkg/imutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

const testImgPath = "testdata/bird.jpg"

const (
	x0, y0, w, h = 65, 35, 30, 20
	delta        = 1e-15
)

type scoreGrayFn[T any] func(img, pat T, offset image.Point) float64

func testScore[T any](t *testing.T, img, pat T, perfectScore float64, scoreFn scoreGrayFn[T]) {
	// score at patch origin
	offset0 := image.Pt(x0, y0)
	score0 := scoreFn(img, pat, offset0)
	assert.InDelta(t, perfectScore, score0, delta)

	// score at offset
	offset1 := image.Pt(x0+1, y0+0)
	score1 := scoreFn(img, pat, offset1)
	assert.Less(t, score1, score0)

	offset2 := image.Pt(x0+0, y0+10)
	score2 := scoreFn(img, pat, offset2)
	assert.Less(t, score2, score0)

	offset3 := image.Pt(x0+1, y0+1)
	score3 := scoreFn(img, pat, offset3)
	assert.Less(t, score3, score0)

	// score at larger offset
	offset4 := image.Pt(x0+3, y0+3)
	score4 := scoreFn(img, pat, offset4)
	assert.Less(t, score4, score3)
}

func Test_ScoreGrayCos(t *testing.T) {
	img := imutil.ToGray(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	require.NoError(t, err)

	ScoreGrayCos(img, pat.(*image.Gray), image.Pt(0, 0))

	testScore(t, img, pat.(*image.Gray), 1, ScoreGrayCos)

	// also resets pat bounds origin to (0,0)
	patCopy := imutil.ToGray(pat.(*image.Gray))

	testScore(t, img, patCopy, 1, ScoreGrayCos)
}

func Test_ScoreGrayCos_Panics(t *testing.T) {
	img := imutil.ToGray(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	require.NoError(t, err)

	assert.Panics(t, func() {
		ScoreGrayCos(img, pat.(*image.Gray), image.Pt(0, -1))
	})
	assert.Panics(t, func() {
		ScoreGrayCos(img, pat.(*image.Gray), image.Pt(-1, 0))
	})
	assert.Panics(t, func() {
		ScoreGrayCos(img, pat.(*image.Gray), image.Pt(-1, -1))
	})

	assert.Panics(t, func() {
		ScoreGrayCos(img, pat.(*image.Gray), image.Pt(0, 200))
	})
	assert.Panics(t, func() {
		ScoreGrayCos(img, pat.(*image.Gray), image.Pt(200, 0))
	})
	assert.Panics(t, func() {
		ScoreGrayCos(img, pat.(*image.Gray), image.Pt(200, 200))
	})
}

func Benchmark_ScoreGrayCos(b *testing.B) {
	img := imutil.ToGray(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	if err != nil {
		b.Fail()
	}

	// make sure pattern lives in a different memory region
	pat = imutil.ToGray(pat.(*image.Gray))

	for i := 0; i < b.N; i++ {
		ScoreGrayCos(img, pat.(*image.Gray), image.Pt(x0, y0))
	}
}

func Test_ScoreRGBCos(t *testing.T) {
	img := imutil.ToRGBA(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	require.NoError(t, err)

	ScoreRGBACos(img, pat.(*image.RGBA), image.Pt(0, 0))

	testScore(t, img, pat.(*image.RGBA), 1, ScoreRGBACos)

	// also resets pat bounds origin to (0,0)
	patCopy := imutil.ToRGBA(pat.(*image.RGBA))

	testScore(t, img, patCopy, 1, ScoreRGBACos)
}

func Test_ScoreRGBACos_Panics(t *testing.T) {
	img := imutil.ToRGBA(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	require.NoError(t, err)

	assert.Panics(t, func() {
		ScoreRGBACos(img, pat.(*image.RGBA), image.Pt(0, -1))
	})
	assert.Panics(t, func() {
		ScoreRGBACos(img, pat.(*image.RGBA), image.Pt(-1, 0))
	})
	assert.Panics(t, func() {
		ScoreRGBACos(img, pat.(*image.RGBA), image.Pt(-1, -1))
	})

	assert.Panics(t, func() {
		ScoreRGBACos(img, pat.(*image.RGBA), image.Pt(0, 200))
	})
	assert.Panics(t, func() {
		ScoreRGBACos(img, pat.(*image.RGBA), image.Pt(200, 0))
	})
	assert.Panics(t, func() {
		ScoreRGBACos(img, pat.(*image.RGBA), image.Pt(200, 200))
	})
}

func Benchmark_ScoreRGBACos(b *testing.B) {
	img := imutil.ToRGBA(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	if err != nil {
		b.Fail()
	}

	// make sure pattern lives in a different memory region
	pat = imutil.ToRGBA(pat.(*image.RGBA))

	for i := 0; i < b.N; i++ {
		ScoreRGBACos(img, pat.(*image.RGBA), image.Pt(x0, y0))
	}
}

func Test_SearchGray(t *testing.T) {
	img := imutil.ToGray(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	require.NoError(t, err)

	x, y, score := SearchGray(img, pat.(*image.Gray))
	assert.Equal(t, 1., score)
	assert.Equal(t, x0, x)
	assert.Equal(t, y0, y)

	// also resets pat bounds origin to (0,0)
	patCopy := imutil.ToGray(pat.(*image.Gray))

	x, y, score = SearchGray(img, patCopy)
	assert.Equal(t, 1., score)
	assert.Equal(t, x0, x)
	assert.Equal(t, y0, y)
}

func Benchmark_SearchGray(b *testing.B) {
	img := imutil.ToGray(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	if err != nil {
		b.Fail()
	}

	// make sure pattern lives in a different memory region
	pat = imutil.ToGray(pat.(*image.Gray))

	for i := 0; i < b.N; i++ {
		SearchGray(img, pat.(*image.Gray))
	}
}

func Test_SearchRGBA(t *testing.T) {
	img := imutil.ToRGBA(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	require.NoError(t, err)

	x, y, score := SearchRGBA(img, pat.(*image.RGBA))
	assert.InDelta(t, 1., score, delta)
	assert.Equal(t, x0, x)
	assert.Equal(t, y0, y)

	// also resets pat bounds origin to (0,0)
	patCopy := imutil.ToRGBA(pat.(*image.RGBA))

	x, y, score = SearchRGBA(img, patCopy)
	assert.InDelta(t, 1., score, delta)
	assert.Equal(t, x0, x)
	assert.Equal(t, y0, y)
}

func Benchmark_SearchRGBA(b *testing.B) {
	img := imutil.ToRGBA(LoadTestImg())
	pat, err := imutil.Sub(img, image.Rect(x0, y0, x0+w, y0+h))
	if err != nil {
		b.Fail()
	}

	// make sure pattern lives in a different memory region
	pat = imutil.ToRGBA(pat.(*image.RGBA))

	for i := 0; i < b.N; i++ {
		SearchRGBA(img, pat.(*image.RGBA))
	}
}
