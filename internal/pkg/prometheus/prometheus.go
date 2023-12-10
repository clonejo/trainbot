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
)
