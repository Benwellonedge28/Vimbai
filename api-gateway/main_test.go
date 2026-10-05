package main

import (
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestHealthCheck(t *testing.T) {
	req, err := http.NewRequest("GET", "/health", nil)
	if err != nil {
		t.Fatal(err)
	}

	rr := httptest.NewRecorder()
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.Write([]byte(`{"status":"healthy"}`))
	})

	handler.ServeHTTP(rr, req)

	if status := rr.Code; status != http.StatusOK {
		t.Errorf("handler returned wrong status code: got %v want %v",
			status, http.StatusOK)
	}

	expected := `{"status":"healthy"}`
	if rr.Body.String() != expected {
		t.Errorf("handler returned unexpected body: got %v want %v",
			rr.Body.String(), expected)
	}
}

func TestBookEventTypeFor(t *testing.T) {
	cases := []struct {
		method, path, want string
	}{
		// writes inside a Book emit events
		{"POST", "/accounting/accounts", "book.resource.created"},
		{"PUT", "/ledger/entries/42", "book.resource.updated"},
		{"PATCH", "/budgets/b1", "book.resource.updated"},
		{"DELETE", "/inventory/items/9", "book.resource.deleted"},
		// domain-specific transaction and journal events
		{"POST", "/accounting/journal-entries", "accounting.journal_entry.created"},
		{"PUT", "/accounting/journal-entries/je-1", "accounting.journal_entry.updated"},
		{"DELETE", "/accounting/journal-entries/je-1", "accounting.journal_entry.deleted"},
		{"POST", "/banking/transactions", "banking.transaction.created"},
		{"PUT", "/banking/transactions/tx-7", "banking.transaction.updated"},
		{"DELETE", "/banking/transactions/tx-7", "banking.transaction.deleted"},
		// reads never emit
		{"GET", "/accounting/journal-entries", ""},
		{"HEAD", "/ledger", ""},
		{"OPTIONS", "/anything", ""},
	}
	for _, tc := range cases {
		got := bookEventTypeFor(tc.method, tc.path)
		if got != tc.want {
			t.Errorf("bookEventTypeFor(%q, %q) = %q, want %q", tc.method, tc.path, got, tc.want)
		}
	}
}
