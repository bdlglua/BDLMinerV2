package main

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"os/signal"
	"strings"
	"syscall"
	"time"
)

type point struct {
	At         int64   `json:"at"`
	Hashrate   float64 `json:"hashrate"`
	Miners     float64 `json:"miners"`
	Average    float64 `json:"average_miners"`
	Samples    int     `json:"samples"`
	IntervalMS int     `json:"interval_ms"`
}

type hourlyPoint struct {
	point
	EndAt int64 `json:"end_at"`
}

type period struct {
	From             int64   `json:"from"`
	To               int64   `json:"to"`
	Samples          int     `json:"samples"`
	ExpectedSamples  int     `json:"expected_samples"`
	AverageHashrate  float64 `json:"average_hashrate"`
	MinimumHashrate  float64 `json:"minimum_hashrate"`
	MaximumHashrate  float64 `json:"maximum_hashrate"`
	VariationPercent float64 `json:"variation_percent"`
	AverageMiners    float64 `json:"average_miners"`
	MinimumMiners    float64 `json:"minimum_miners"`
	MaximumMiners    float64 `json:"maximum_miners"`
	FirstAt          int64   `json:"first_at"`
	LastAt           int64   `json:"last_at"`
}

type namedTotal struct {
	Name     string  `json:"name"`
	Miners   int     `json:"miners"`
	Hashrate float64 `json:"hashrate"`
}

type fleetBoard struct {
	Board   string `json:"board"`
	Online  int    `json:"online"`
	Offline int    `json:"offline"`
}

type modelTotal struct {
	Board    string  `json:"board"`
	Model    string  `json:"model"`
	Miners   int     `json:"miners"`
	Hashrate float64 `json:"hashrate"`
}

type summary struct {
	At          int64             `json:"at"`
	Miners      int               `json:"miners"`
	Hashrate    float64           `json:"hashrate"`
	Coins       []namedTotal      `json:"coins"`
	Boards      []namedTotal      `json:"boards"`
	Versions    []namedTotal      `json:"versions"`
	Fleet       []fleetBoard      `json:"fleet"`
	Models      []modelTotal      `json:"models"`
	History     []hourlyPoint     `json:"history"`
	CurrentHour hourlyPoint       `json:"current_hour"`
	Recent      []point           `json:"recent"`
	Record      namedTotal        `json:"record"`
	Pools       []namedTotal      `json:"pools"`
	Periods     map[string]period `json:"periods"`
}

func sampleSummary(now time.Time) summary {
	end := now.Truncate(5 * time.Minute)
	at := end.UnixMilli()
	recent := make([]point, 0, 7)
	for i := 6; i >= 0; i-- {
		miners := 7 + (i % 3)
		recent = append(recent, point{
			At:         at - int64(i)*5*60*1000,
			Hashrate:   float64(2_400_000 + (i%4)*100_000),
			Miners:     float64(miners),
			Average:    float64(miners),
			Samples:    1,
			IntervalMS: 5 * 60 * 1000,
		})
	}

	history := make([]hourlyPoint, 0, 168)
	for i := 168; i >= 1; i-- {
		endAt := at - int64(i)*time.Hour.Milliseconds()
		miners := 7 + (i % 3)
		history = append(history, hourlyPoint{
			point: point{
				At:         endAt - time.Hour.Milliseconds(),
				Hashrate:   float64(2_300_000 + (i%5)*100_000),
				Miners:     float64(miners),
				Average:    float64(miners),
				Samples:    12,
				IntervalMS: int(time.Hour.Milliseconds()),
			},
			EndAt: endAt,
		})
	}

	nowHour := hourlyPoint{
		point: point{
			At:         at - time.Hour.Milliseconds(),
			Hashrate:   2_500_000,
			Miners:     8,
			Average:    8,
			Samples:    12,
			IntervalMS: int(time.Hour.Milliseconds()),
		},
		EndAt: at,
	}
	periods := make(map[string]period, 4)
	for _, hours := range []int{1, 6, 24, 168} {
		expected := hours * 12
		samples := expected
		if hours == 168 {
			expected = 168 * 12
			samples = expected
		}
		periods[fmt.Sprint(hours)] = period{
			From:             at - int64(hours)*time.Hour.Milliseconds(),
			To:               at,
			Samples:          samples,
			ExpectedSamples:  expected,
			AverageHashrate:  2_500_000,
			MinimumHashrate:  2_300_000,
			MaximumHashrate:  2_800_000,
			VariationPercent: 6.5,
			AverageMiners:    8,
			MinimumMiners:    7,
			MaximumMiners:    9,
			FirstAt:          at - int64(hours)*time.Hour.Milliseconds(),
			LastAt:           at,
		}
	}

	return summary{
		At:       at,
		Miners:   8,
		Hashrate: 2_500_000,
		Coins: []namedTotal{
			{Name: "BTC", Miners: 6, Hashrate: 2_000_000},
			{Name: "BCH", Miners: 2, Hashrate: 500_000},
		},
		Boards: []namedTotal{
			{Name: "m5stickc-plus2", Miners: 5, Hashrate: 1_500_000},
			{Name: "lilygo-t-display-s3", Miners: 3, Hashrate: 1_000_000},
		},
		Versions: []namedTotal{
			{Name: "2.5.6", Miners: 8, Hashrate: 2_500_000},
		},
		Fleet: []fleetBoard{
			{Board: "m5stickc-plus2", Online: 5},
			{Board: "lilygo-t-display-s3", Online: 3},
		},
		Models: []modelTotal{
			{Board: "m5stickc-plus2", Model: "M5StickC Plus2", Miners: 5, Hashrate: 1_500_000},
			{Board: "lilygo-t-display-s3", Model: "LilyGo T-Display S3", Miners: 3, Hashrate: 1_000_000},
		},
		History:     history,
		CurrentHour: nowHour,
		Recent:      recent,
		Record:      namedTotal{Name: "24h", Miners: 8, Hashrate: 2_800_000},
		Pools: []namedTotal{
			{Name: "public-pool.io", Miners: 4, Hashrate: 1_200_000},
			{Name: "other", Miners: 2, Hashrate: 800_000},
			{Name: "unknown", Miners: 2, Hashrate: 500_000},
		},
		Periods: periods,
	}
}

func method(w http.ResponseWriter, r *http.Request, allowed string) bool {
	w.Header().Set("Allow", allowed)
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return false
	}
	return true
}

func handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/summary", func(w http.ResponseWriter, r *http.Request) {
		if !method(w, r, http.MethodGet) {
			return
		}
		w.Header().Set("Content-Type", "application/json; charset=utf-8")
		w.Header().Set("Cache-Control", "no-store")
		w.Header().Set("Access-Control-Allow-Origin", "*")
		if err := json.NewEncoder(w).Encode(sampleSummary(time.Now())); err != nil {
			log.Printf("encode summary: %v", err)
		}
	})
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {
		if !method(w, r, http.MethodGet) {
			return
		}
		w.Header().Set("Content-Type", "application/json; charset=utf-8")
		_, _ = w.Write([]byte("{\"status\":\"ok\",\"mode\":\"synthetic prototype\"}\n"))
	})
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		http.NotFound(w, r)
	})
	return mux
}

func validateLoopback(address string) error {
	host, _, err := net.SplitHostPort(address)
	if err != nil {
		return fmt.Errorf("invalid listen address: %w", err)
	}
	if strings.EqualFold(host, "localhost") {
		return nil
	}
	ip := net.ParseIP(host)
	if ip == nil || !ip.IsLoopback() {
		return errors.New("prototype must bind to localhost or a loopback IP")
	}
	return nil
}

func run() error {
	address := flag.String("addr", "127.0.0.1:8787", "loopback address for the local prototype")
	flag.Parse()
	if err := validateLoopback(*address); err != nil {
		return err
	}

	server := &http.Server{
		Addr:              *address,
		Handler:           handler(),
		ReadHeaderTimeout: 5 * time.Second,
	}
	listener, err := net.Listen("tcp", *address)
	if err != nil {
		return fmt.Errorf("listen on %s: %w", *address, err)
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	go func() {
		<-ctx.Done()
		shutdown, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		if err := server.Shutdown(shutdown); err != nil {
			log.Printf("shutdown server: %v", err)
		}
	}()

	log.Printf("local synthetic statistics API listening on http://%s/api/summary", listener.Addr())
	if err := server.Serve(listener); err != nil && !errors.Is(err, http.ErrServerClosed) {
		return fmt.Errorf("serve statistics API: %w", err)
	}
	return nil
}

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}
