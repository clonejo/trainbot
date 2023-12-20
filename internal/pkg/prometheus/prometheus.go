// Package prometheus configures, initializes and serves global application prometheus metrics.
package prometheus

import (
	"net/http"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"github.com/prometheus/client_golang/prometheus/promhttp"
)

func Init(prometheusListen string) {
	http.Handle("/metrics", promhttp.Handler())
	go http.ListenAndServe(prometheusListen, nil)
}

func RecordFrameDisposition(disposition string) {
	frameDispositions.WithLabelValues(disposition).Inc()
}
func RecordSequenceLength(length int) {
	sequenceLength.Set(float64(length))
}

func RecordFitAndStitchResult(result string) {
	fitAndStitchResult.WithLabelValues(result).Inc()
}
func RecordBrightnessContrast(avg float64, avgDev float64) {
	brightnessAvg.Observe(avg)
	brightnessAvgDev.Observe(avgDev)
}

var (
	frameDispositions = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "trainbot_frame_dispositions_total",
			Help: "How frames were used",
		},
		[]string{"disposition"},
	)
	sequenceLength = promauto.NewGauge(
		prometheus.GaugeOpts{
			Name: "trainbot_sequence_length",
			Help: "Current number of frames stored.",
		},
	)
	fitAndStitchResult = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "trainbot_fit_and_stitch_results_total",
			Help: "Results from fitAndStitch(). Eg. train detected, unable to fit.",
		},
		[]string{"result"},
	)
	brightnessAvg = promauto.NewHistogram(
		prometheus.HistogramOpts{
			Name:    "trainbot_brightness_avg",
			Buckets: prometheus.ExponentialBucketsRange(0.0005, 1.0, 10),
		},
	)
	brightnessAvgDev = promauto.NewHistogram(
		prometheus.HistogramOpts{
			Name:    "trainbot_brightness_avgdev",
			Buckets: prometheus.ExponentialBucketsRange(0.0005, 1.0, 10),
		},
	)
)
