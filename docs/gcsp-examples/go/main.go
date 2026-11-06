// GCSP/1 reference handler in Go (docs/gcsp.md), standard library only.
//
// Replace the in-memory slices with queries against your own tables. The important parts are
// the cursor (last updated_at + id), ascending order with a stable tie-break, tombstones for
// deleted rows, and returning next_cursor on the last page too.
//
//	go run . -addr 127.0.0.1:8787 -token dev-token
//	globalcontact source check http://127.0.0.1:8787 --token dev-token
package main

import (
	"crypto/subtle"
	"encoding/base64"
	"encoding/json"
	"flag"
	"log"
	"net/http"
	"sort"
	"strconv"
	"strings"
	"time"
)

type Identity struct {
	Kind     string `json:"kind"`
	Value    string `json:"value"`
	Verified bool   `json:"verified,omitempty"`
}

type Person struct {
	ExternalID  string         `json:"external_id"`
	UpdatedAt   time.Time      `json:"updated_at"`
	Deleted     bool           `json:"deleted,omitempty"`
	DisplayName string         `json:"display_name,omitempty"`
	Identities  []Identity     `json:"identities,omitempty"`
	Attributes  map[string]any `json:"attributes,omitempty"`
	URL         string         `json:"url,omitempty"`
}

type Event struct {
	ExternalID       string         `json:"external_id"`
	PersonExternalID string         `json:"person_external_id,omitempty"`
	OccurredAt       time.Time      `json:"occurred_at"`
	UpdatedAt        time.Time      `json:"updated_at"`
	Deleted          bool           `json:"deleted,omitempty"`
	Type             string         `json:"type,omitempty"`
	Title            string         `json:"title,omitempty"`
	BodyMarkdown     string         `json:"body_markdown,omitempty"`
	URL              string         `json:"url,omitempty"`
	Attributes       map[string]any `json:"attributes,omitempty"`
}

type page[T any] struct {
	Items      []T     `json:"items"`
	NextCursor *string `json:"next_cursor"`
	HasMore    bool    `json:"has_more"`
}

var manifest = map[string]any{
	"protocol":     "gcsp/1",
	"source":       map[string]any{"id": "my-backend", "name": "My Backend", "url": "https://example.com"},
	"capabilities": []string{"people", "events", "person_detail"},
	// Only declared attributes are kept by OutreachDex; declare exactly what you want to share.
	"attributes": []map[string]any{
		{"key": "plan", "label": "Plan", "type": "string"},
		{"key": "signup_at", "label": "Signed up", "type": "datetime"},
	},
	"event_types": []map[string]any{
		{"type": "signup", "label": "Signed up"},
		{"type": "feedback_submitted", "label": "Feedback submitted"},
	},
}

// cursor encodes the position after the last returned row: its updated_at and id.
type cursor struct {
	At time.Time
	ID string
}

func encode(at time.Time, id string) *string {
	s := base64.RawURLEncoding.EncodeToString([]byte(at.UTC().Format(time.RFC3339Nano) + "|" + id))
	return &s
}

func decode(raw string) (*cursor, bool) {
	if raw == "" {
		return nil, true
	}
	b, err := base64.RawURLEncoding.DecodeString(raw)
	if err != nil {
		return nil, false
	}
	at, id, ok := strings.Cut(string(b), "|")
	t, err := time.Parse(time.RFC3339Nano, at)
	if !ok || err != nil {
		return nil, false
	}
	return &cursor{t, id}, true
}

// paginate is the in-memory version of:
//
//	SELECT ... WHERE (updated_at, id) > (?, ?) ORDER BY updated_at, id LIMIT ?+1
func paginate[T any](rows []T, key func(T) (time.Time, string), after *cursor, limit int, previous string) page[T] {
	sort.Slice(rows, func(i, j int) bool {
		ai, ii := key(rows[i])
		aj, ij := key(rows[j])
		return ai.Before(aj) || (ai.Equal(aj) && ii < ij)
	})
	var rest []T
	for _, r := range rows {
		at, id := key(r)
		if after == nil || at.After(after.At) || (at.Equal(after.At) && id > after.ID) {
			rest = append(rest, r)
		}
	}
	p := page[T]{Items: []T{}}
	if len(rest) > limit {
		p.HasMore = true
		rest = rest[:limit]
	}
	p.Items = append(p.Items, rest...)
	if n := len(rest); n > 0 {
		at, id := key(rest[n-1])
		p.NextCursor = encode(at, id)
	} else if previous != "" {
		// Nothing new: hand the same position back so the client resumes from it next time.
		p.NextCursor = &previous
	}
	return p
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func fail(w http.ResponseWriter, status int, code, message string) {
	writeJSON(w, status, map[string]any{"error": map[string]string{"code": code, "message": message}})
}

func main() {
	addr := flag.String("addr", "127.0.0.1:8787", "listen address")
	token := flag.String("token", "dev-token", "read-only token for OutreachDex")
	flag.Parse()

	now := time.Now().UTC().Truncate(time.Second)
	people := []Person{
		{ExternalID: "u_1", UpdatedAt: now.Add(-48 * time.Hour), DisplayName: "Lin Wei",
			Identities: []Identity{{Kind: "email", Value: "lin@example.com", Verified: true}},
			Attributes: map[string]any{"plan": "pro", "signup_at": now.Add(-90 * 24 * time.Hour).Format(time.RFC3339)},
			URL:        "https://example.com/admin/users/1"},
		{ExternalID: "u_2", UpdatedAt: now.Add(-24 * time.Hour), DisplayName: "Chen Zhuo",
			Identities: []Identity{{Kind: "email", Value: "chen@example.com"}},
			Attributes: map[string]any{"plan": "free"}},
		{ExternalID: "u_3", UpdatedAt: now.Add(-time.Hour), Deleted: true}, // a deleted account
	}
	events := []Event{
		{ExternalID: "e_1", PersonExternalID: "u_1", OccurredAt: now.Add(-47 * time.Hour), Type: "signup", Title: "Created an account"},
		{ExternalID: "e_2", PersonExternalID: "u_1", OccurredAt: now.Add(-2 * time.Hour), Type: "feedback_submitted",
			Title: "Feedback: export fails", BodyMarkdown: "> PDF export just keeps spinning.", Attributes: map[string]any{"status": "open"}},
	}
	for i := range events {
		events[i].UpdatedAt = events[i].OccurredAt
	}

	auth := func(next http.HandlerFunc) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			got := strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")
			if subtle.ConstantTimeCompare([]byte(got), []byte(*token)) != 1 {
				fail(w, http.StatusUnauthorized, "unauthorized", "missing or wrong token")
				return
			}
			next(w, r)
		}
	}
	limitOf := func(r *http.Request) int {
		n, err := strconv.Atoi(r.URL.Query().Get("limit"))
		if err != nil || n <= 0 {
			return 100
		}
		return min(n, 500)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /manifest", auth(func(w http.ResponseWriter, r *http.Request) { writeJSON(w, 200, manifest) }))
	mux.HandleFunc("GET /people", auth(func(w http.ResponseWriter, r *http.Request) {
		raw := r.URL.Query().Get("cursor")
		after, ok := decode(raw)
		if !ok {
			fail(w, 400, "invalid_cursor", "cursor not recognized; sync from the start")
			return
		}
		writeJSON(w, 200, paginate(people, func(p Person) (time.Time, string) { return p.UpdatedAt, p.ExternalID }, after, limitOf(r), raw))
	}))
	mux.HandleFunc("GET /events", auth(func(w http.ResponseWriter, r *http.Request) {
		raw := r.URL.Query().Get("cursor")
		after, ok := decode(raw)
		if !ok {
			fail(w, 400, "invalid_cursor", "cursor not recognized; sync from the start")
			return
		}
		writeJSON(w, 200, paginate(events, func(e Event) (time.Time, string) { return e.UpdatedAt, e.ExternalID }, after, limitOf(r), raw))
	}))
	mux.HandleFunc("GET /people/{id}", auth(func(w http.ResponseWriter, r *http.Request) {
		for _, p := range people {
			if p.ExternalID == r.PathValue("id") {
				writeJSON(w, 200, p)
				return
			}
		}
		fail(w, 404, "not_found", "no such person")
	}))
	log.Printf("GCSP/1 on http://%s (token %s)", *addr, *token)
	log.Fatal(http.ListenAndServe(*addr, mux))
}
