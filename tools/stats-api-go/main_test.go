package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestSummaryEndpointMatchesPublicSiteShape(t *testing.T) {
	recorder := httptest.NewRecorder()
	request := httptest.NewRequest(http.MethodGet, "/api/summary", nil)
	handler().ServeHTTP(recorder, request)

	if recorder.Code != http.StatusOK {
		t.Fatalf("status = %d, want %d", recorder.Code, http.StatusOK)
	}
	if got := recorder.Header().Get("Content-Type"); !strings.HasPrefix(got, "application/json") {
		t.Fatalf("Content-Type = %q, want application/json", got)
	}
	if got := recorder.Header().Get("Cache-Control"); got != "no-store" {
		t.Fatalf("Cache-Control = %q, want no-store", got)
	}
	if got := recorder.Header().Get("Access-Control-Allow-Origin"); got != "*" {
		t.Fatalf("Access-Control-Allow-Origin = %q, want * for local browser preview", got)
	}

	var body map[string]json.RawMessage
	if err := json.Unmarshal(recorder.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode summary: %v", err)
	}
	for _, key := range []string{
		"at", "miners", "hashrate", "coins", "boards", "versions", "fleet",
		"models", "history", "current_hour", "recent", "record", "pools", "periods",
	} {
		if _, ok := body[key]; !ok {
			t.Errorf("summary is missing field %q", key)
		}
	}

	var value summary
	if err := json.Unmarshal(recorder.Body.Bytes(), &value); err != nil {
		t.Fatalf("decode summary contract: %v", err)
	}
	if value.Miners <= 0 || value.Hashrate <= 0 || len(value.Recent) == 0 || len(value.History) == 0 {
		t.Fatalf("expected non-empty synthetic chart data, got %+v", value)
	}
	for _, hours := range []string{"1", "6", "24", "168"} {
		if _, ok := value.Periods[hours]; !ok {
			t.Errorf("summary is missing period %q", hours)
		}
	}
}

func TestSummaryTimestampsStayWithinExpectedWindows(t *testing.T) {
	now := time.Now()
	value := sampleSummary(now)
	if delta := now.UnixMilli() - value.At; delta < 0 || delta > 5*time.Minute.Milliseconds() {
		t.Fatalf("summary timestamp is unexpectedly far from now: delta=%dms", delta)
	}
	if got := value.Recent[len(value.Recent)-1].At; got > value.At {
		t.Fatalf("latest recent point %d is after summary time %d", got, value.At)
	}
	if got := value.History[len(value.History)-1].EndAt; got != value.At-time.Hour.Milliseconds() {
		t.Fatalf("last completed hourly bucket ends at %d, want %d", got, value.At-time.Hour.Milliseconds())
	}
}

func TestOnlyExpectedReadEndpointsAreAvailable(t *testing.T) {
	for _, path := range []string{"/", "/api/remote/report", "/api/summary/"} {
		recorder := httptest.NewRecorder()
		handler().ServeHTTP(recorder, httptest.NewRequest(http.MethodGet, path, nil))
		if recorder.Code != http.StatusNotFound {
			t.Errorf("GET %s status = %d, want %d", path, recorder.Code, http.StatusNotFound)
		}
	}

	for _, path := range []string{"/api/summary", "/healthz"} {
		recorder := httptest.NewRecorder()
		handler().ServeHTTP(recorder, httptest.NewRequest(http.MethodPost, path, nil))
		if recorder.Code != http.StatusMethodNotAllowed {
			t.Errorf("POST %s status = %d, want %d", path, recorder.Code, http.StatusMethodNotAllowed)
		}
		if got := recorder.Header().Get("Allow"); got != http.MethodGet {
			t.Errorf("POST %s Allow = %q, want GET", path, got)
		}
	}
}

func TestListenAddressMustBeLoopback(t *testing.T) {
	for _, address := range []string{"127.0.0.1:8787", "[::1]:8787", "localhost:8787"} {
		if err := validateLoopback(address); err != nil {
			t.Errorf("validateLoopback(%q): %v", address, err)
		}
	}
	for _, address := range []string{"0.0.0.0:8787", "192.0.2.1:8787", "invalid"} {
		if err := validateLoopback(address); err == nil {
			t.Errorf("validateLoopback(%q) unexpectedly succeeded", address)
		}
	}
}
